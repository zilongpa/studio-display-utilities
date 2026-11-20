"""Summarises tt-log.jsonl from tools/mac-truetone.swift.

    python analyze-truetone.py tt-log.jsonl

For each lamp step it takes the last sample (the settled state) and prints every recorded value that
differs between steps, so the values that follow the ambient light stand out. With --timeline KEY it
prints how one value moved over time instead (for the speed of the change).
"""
import json
import sys


def flatten(value, path=""):
    if isinstance(value, dict):
        out = {}
        for key, item in value.items():
            out.update(flatten(item, f"{path}.{key}" if path else key))
        return out
    return {path: value if isinstance(value, (int, float, str, bool)) else json.dumps(value)}


def main():
    arguments = [a for a in sys.argv[1:] if not a.startswith("--")]
    samples = [json.loads(line) for line in open(arguments[0], encoding="utf-8") if line.strip()]
    if "--timeline" in sys.argv:
        key = sys.argv[sys.argv.index("--timeline") + 1]
        last = None
        for sample in samples:
            value = flatten(sample).get(key)
            if value != last:
                print(f"{sample.get('t', 0):8.1f}  {sample.get('step', ''):14}  {value}")
                last = value
        return

    settled = {}  # step -> last sample of that step, in order of appearance
    for sample in samples:
        settled[sample.get("step", "")] = flatten(sample)
    steps = list(settled)
    keys = sorted({key for flat in settled.values() for key in flat} - {"t", "step"})
    varying = [key for key in keys if len({str(settled[step].get(key)) for step in steps}) > 1]
    print(f"{len(samples)} samples, {len(steps)} steps, {len(keys)} values recorded, {len(varying)} of them differ between steps\n")
    for key in varying:
        print(key)
        for step in steps:
            text = str(settled[step].get(key))
            print(f"    {step or '(no step)':14}  {text if len(text) <= 160 else text[:160] + ' ...'}")
        print()
    constant = [key for key in keys if key not in varying]
    print("the same in every step: " + ", ".join(constant))


if __name__ == "__main__":
    main()
