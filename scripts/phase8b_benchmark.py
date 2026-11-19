"""Release stage timings and per-child peak RSS; no production instrumentation.

The small Rust drivers include the recorded Git sources/current sources and
observe private capacities. RSS comes from a fresh measuring process per run,
so resource.getrusage(CHILDREN) never combines multiple benchmark children.
"""

import argparse
import io
import json
from pathlib import Path
import statistics
import subprocess
import sys
import tarfile

import phase8b_regression as regression
import verify_corpus as v


def cases():
    def dsl(generation, output):
        return f"generate {{\n{generation}\n}}\ninput {{\n{output}\n}}\n"
    result = {
        "integer-array": dsl("A[400000]\nfor i in 1..=400000 {\nA[i] in 1234567890..=1234567890\n}", "A"),
        "blank-lines": dsl("", "for i in 1..=1000000 {\n" + "\n" * 8 + "}"),
        "repeated-string": dsl('S : string[1000000]\nfor i in 1..=1000000 {\nS[i] = \'a\'\n}',
                               "for i in 1..=8 {\nS\n}"),
        "grid": dsl("G[1000] : string[1000]\nfor i in 1..=1000 {\nfor j in 1..=1000 {\nG[i][j] = '.'\n}\n}",
                    "for i in 1..=1000 {\nG[i]\n}"),
    }
    for name, problem, profile, builder in (
        ("many-queries", ("abc241", "d"), "official-Q-all-query2", v.batch1_profiles),
        ("abc413-e", ("abc413", "e"), "official-T-total", v.batch2.profiles),
    ):
        source = (v.CORPUS / problem[0] / problem[1] / "generator.gen").read_text()
        result[name] = next(p.source for p in builder(problem, source) if p.name == profile)
    return result


def driver(root, before):
    modules = ("ast", "error", "generator", "parser", "validation") + (() if before else ("runtime",))
    declarations = "\n".join(f'#[path = {json.dumps(str(root / "src" / (m + ".rs")))}] mod {m};' for m in modules)
    metrics = ("let tokens = plan.tokens.len();\n"
               "let token_capacity = plan.tokens.capacity();\n"
               "let row_capacity = plan.row_ends.capacity();\n"
               "let retained = token_capacity * size_of::<Token>() + row_capacity * size_of::<u32>();\n"
               "let loops = 0;" if before else
               "let tokens = plan.counts.tokens;\nlet token_capacity = 0;\nlet row_capacity = 0;\n"
               "let retained = 0;\nlet loops = plan.loop_values.capacity() * size_of::<i64>();")
    return declarations + "\npub use error::Error;\nmod output {\n" + (
        f'include!({json.dumps(str(root / "src/output.rs"))});\n') + """
    pub(crate) fn measured(statements: &[BoundInputStmt<'_>], values: &[Value])
        -> Result<(String, u128, u128, usize, usize, usize, usize, usize), Error> {
        let started = std::time::Instant::now();
        let plan = validate(statements, values)?;
        let validation_ns = started.elapsed().as_nanos();
        METRICS
        let started = std::time::Instant::now();
        let output = FORMAT;
        Ok((output, validation_ns, started.elapsed().as_nanos(), tokens,
            token_capacity, row_capacity, retained, loops))
    }
}
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let source = std::fs::read_to_string(path).unwrap();
    let total = std::time::Instant::now();
    let started = std::time::Instant::now();
    let program = validation::validate(parser::parse(&source).unwrap()).unwrap();
    let parse_validate_ns = started.elapsed().as_nanos();
    let started = std::time::Instant::now();
    let values = generator::generate(&program.generate, program.slot_count, 42).unwrap();
    let generation_ns = started.elapsed().as_nanos();
    let (output, input_ns, format_ns, tokens, token_capacity, row_capacity, intermediate_bytes, loop_bytes)
        = output::measured(&program.input, &values).unwrap();
    let total_ns = total.elapsed().as_nanos();
    let array_bytes: usize = values.iter().map(|v| match v {
        generator::Value::Array(a) => a.capacity() * size_of::<Option<i64>>(), _ => 0
    }).sum();
    let character_bytes: usize = values.iter().map(|v| match v {
        generator::Value::String(s) | generator::Value::StringArray(s) => s.cells.capacity(), _ => 0
    }).sum();
    let value_bytes = values.capacity() * size_of::<generator::Value>();
    let output_bytes = output.len();
    let output_capacity = output.capacity();
    let fingerprint = output.bytes().fold(0xcbf29ce484222325u64,
        |state, byte| (state ^ u64::from(byte)).wrapping_mul(0x100000001b3));
    println!("{{\\"parse_validate_ns\\":{parse_validate_ns},\\"generation_ns\\":{generation_ns},\\"input_ns\\":{input_ns},\\"format_ns\\":{format_ns},\\"total_ns\\":{total_ns},\\"tokens\\":{tokens},\\"output_bytes\\":{output_bytes},\\"output_capacity\\":{output_capacity},\\"token_capacity\\":{token_capacity},\\"row_capacity\\":{row_capacity},\\"intermediate_bytes\\":{intermediate_bytes},\\"loop_bytes\\":{loop_bytes},\\"array_bytes\\":{array_bytes},\\"character_bytes\\":{character_bytes},\\"value_bytes\\":{value_bytes},\\"fingerprint\\":{fingerprint}}}");
    std::hint::black_box(output);
}
""".replace("METRICS", metrics).replace("FORMAT", "plan.format()" if before else "plan.format()?")


MEASURE = """
import json, subprocess, sys, time
try:
    import resource
except ImportError:
    resource = None
started = time.perf_counter()
p = subprocess.run(sys.argv[1:], capture_output=True, check=True)
d = json.loads(p.stdout)
d['process_seconds'] = time.perf_counter() - started
d['peak_rss_bytes'] = (resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss *
                       (1 if sys.platform == 'darwin' else 1024)) if resource else None
print(json.dumps(d))
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=regression.MANIFEST)
    parser.add_argument("--directory", type=Path, default=v.ROOT / "target/phase8b-benchmark")
    parser.add_argument("--report", type=Path, default=v.ROOT / "target/phase8b-benchmark.json")
    parser.add_argument("--repetitions", type=int, default=5)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    args.directory.mkdir(parents=True, exist_ok=True)
    reference = args.directory / "reference"
    reference.mkdir(exist_ok=True)
    raw = subprocess.check_output(["git", "archive", manifest["baseline_commit"]], cwd=v.ROOT)
    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
        archive.extractall(reference, filter="data")
    binaries = {}
    for name, root in (("before", reference), ("after", v.ROOT)):
        source = args.directory / f"{name}.rs"
        source.write_text(driver(root, name == "before"))
        binary = args.directory / name
        subprocess.run(["rustc", "--edition=2024", "-O", "-A", "dead_code", str(source), "-o", str(binary)], check=True)
        binaries[name] = binary
    records = []
    for name, source in cases().items():
        path = args.directory / f"{name}.gen"
        path.write_text(source)
        observations = {"before": [], "after": []}
        for repetition in range(args.repetitions):
            for variant in (("before", "after") if repetition % 2 == 0 else ("after", "before")):
                raw = subprocess.check_output([sys.executable, "-c", MEASURE, str(binaries[variant]), str(path)])
                observations[variant].append(json.loads(raw))
        old, new = observations["before"][0], observations["after"][0]
        for samples in observations.values():
            for sample in samples:
                v.require((sample["output_bytes"], sample["tokens"], sample["fingerprint"]) ==
                          (old["output_bytes"], old["tokens"], old["fingerprint"]), "benchmark output changed")
        medians = {variant: {key: statistics.median(s[key] for s in samples)
                             for key in old if isinstance(old[key], (int, float))}
                   for variant, samples in observations.items()}
        records.append({"case": name, "source": source, "source_sha256": regression.digest(source.encode()),
                        "seed": 42, "samples": observations, "medians": medians})
        print(name, "RSS", old["peak_rss_bytes"], "->", new["peak_rss_bytes"], flush=True)
    regression.write_json(args.report, {"profile": "rustc -O, isolated stage driver",
                          "platform": sys.platform, "repetitions": args.repetitions,
                          "rss": "OS ru_maxrss of one fresh child; bytes (Darwin), KiB converted to bytes (Linux)",
                          "timing": "generation excludes parse/validation and output; input and format measured separately; no stdout payload I/O",
                          "records": records})


if __name__ == "__main__":
    main()
