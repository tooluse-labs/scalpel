#!/usr/bin/env python3
"""Drift check between frozen pdbg_shim.h and hand-written raw Rust bindings.

M0 intentionally avoids bindgen and MuPDF headers, so this script is the ABI
guard for the frozen pdbg_shim.h seam. It compares enum discriminants, exported
function signatures, callback typedefs, and public struct field order/types.
"""

from __future__ import annotations

import pathlib
import re


ROOT = pathlib.Path(__file__).resolve().parents[1]
HEADER = ROOT / "crates/pdbg-shim/include/pdbg_shim.h"
RAW = ROOT / "crates/pdbg-shim/src/raw.rs"

HEADER_ENUM = re.compile(r"typedef\s+enum\s+(\w+)\s*\{(?P<body>.*?)\}\s*\1\s*;", re.S)
RUST_ENUM = re.compile(r"pub\s+enum\s+(\w+)\s*\{(?P<body>.*?)\}", re.S)
VARIANT = re.compile(r"\b([A-Z][A-Z0-9_]+)\s*=\s*([0-9]+)")
HEADER_STRUCT = re.compile(r"typedef\s+struct\s+(\w+)\s*\{(?P<body>.*?)\}\s*\1\s*;", re.S)
RUST_STRUCT = re.compile(r"pub\s+struct\s+(\w+)\s*\{(?P<body>.*?)\n\}", re.S)
HEADER_FN = re.compile(r"(?P<ret>[A-Za-z_][\w\s\*]*?)\s*(?P<name>pdbg_\w+)\s*\((?P<args>.*?)\)\s*;", re.S)
RUST_FN = re.compile(r"pub\s+fn\s+(?P<name>pdbg_\w+)\s*\((?P<args>.*?)\)\s*(?:->\s*(?P<ret>[^;]+))?;", re.S)
HEADER_CALLBACK = re.compile(
    r"typedef\s+(?P<ret>\w+)\s+\(\*(?P<name>pdbg_\w+)\)\((?P<args>.*?)\)\s*;"
)
RUST_CALLBACK = re.compile(r"pub\s+type\s+(?P<name>pdbg_\w+)\s*=\s*(?P<body>.*?);", re.S)


def enum_map(text: str, pattern: re.Pattern[str]) -> dict[str, dict[str, int]]:
    out: dict[str, dict[str, int]] = {}
    for match in pattern.finditer(text):
        name = match.group(1)
        body = match.group("body")
        out[name] = {variant: int(value) for variant, value in VARIANT.findall(body)}
    return out


def c_structs(text: str) -> dict[str, list[tuple[str, str]]]:
    structs: dict[str, list[tuple[str, str]]] = {}
    for match in HEADER_STRUCT.finditer(text):
        fields: list[tuple[str, str]] = []
        for statement in statements(match.group("body")):
            field_type, field_name = c_decl_type_and_name(statement)
            fields.append((field_name, normalize_c_type(field_type)))
        structs[match.group(1)] = fields
    return structs


def rust_structs(text: str) -> dict[str, list[tuple[str, str]]]:
    structs: dict[str, list[tuple[str, str]]] = {}
    for match in RUST_STRUCT.finditer(text):
        fields: list[tuple[str, str]] = []
        for line in match.group("body").splitlines():
            line = line.strip().rstrip(",")
            if not line.startswith("pub "):
                continue
            name, raw_type = line.removeprefix("pub ").split(":", 1)
            fields.append((name.strip(), normalize_rust_type(raw_type.strip())))
        structs[match.group(1)] = fields
    return structs


def c_functions(text: str) -> dict[str, tuple[str, list[str]]]:
    text = strip_typedefs(text)
    functions: dict[str, tuple[str, list[str]]] = {}
    for match in HEADER_FN.finditer(text):
        ret = normalize_c_return_type(match.group("ret"))
        args = [normalize_c_type(c_arg_type(arg)) for arg in split_args(match.group("args"))]
        functions[match.group("name")] = (ret, args)
    return functions


def rust_functions(text: str) -> dict[str, tuple[str, list[str]]]:
    functions: dict[str, tuple[str, list[str]]] = {}
    for match in RUST_FN.finditer(text):
        ret = normalize_rust_type((match.group("ret") or "()").strip())
        args = []
        for arg in split_args(match.group("args")):
            if ":" not in arg:
                raise ValueError(f"cannot parse Rust argument for {match.group('name')}: {arg}")
            args.append(normalize_rust_type(arg.split(":", 1)[1].strip()))
        functions[match.group("name")] = (ret, args)
    return functions


def c_callbacks(text: str) -> dict[str, tuple[str, list[str]]]:
    callbacks: dict[str, tuple[str, list[str]]] = {}
    for match in HEADER_CALLBACK.finditer(text):
        callbacks[match.group("name")] = (
            normalize_c_type(match.group("ret")),
            [normalize_c_type(c_arg_type(arg)) for arg in split_args(match.group("args"))],
        )
    return callbacks


def rust_callbacks(text: str) -> dict[str, tuple[str, list[str]]]:
    callbacks: dict[str, tuple[str, list[str]]] = {}
    for match in RUST_CALLBACK.finditer(text):
        body = " ".join(match.group("body").split())
        fn_match = re.search(r'extern "C" fn\s*\((?P<args>.*?)\)\s*->\s*(?P<ret>[^>]+)>?$', body)
        if not fn_match:
            continue
        callbacks[match.group("name")] = (
            normalize_rust_type(fn_match.group("ret").strip()),
            [normalize_rust_type(arg.split(":", 1)[1].strip()) for arg in split_args(fn_match.group("args"))],
        )
    return callbacks


def statements(body: str) -> list[str]:
    return [line.strip() for line in body.split(";") if line.strip()]


def split_args(args: str) -> list[str]:
    args = " ".join(args.split())
    if not args or args == "void":
        return []
    return [arg.strip() for arg in args.split(",") if arg.strip()]


def c_decl_type_and_name(decl: str) -> tuple[str, str]:
    decl = " ".join(decl.split())
    array_match = re.match(r"(?P<type>.+?)\s+(?P<name>\w+)\[(?P<len>\d+)\]$", decl)
    if array_match:
        return f"{array_match.group('type')}[{array_match.group('len')}]", array_match.group("name")
    match = re.match(r"(?P<type>.+?)(?P<pointers>\*+)?\s*(?P<name>\w+)$", decl)
    if not match:
        raise ValueError(f"cannot parse C declaration: {decl}")
    field_type = (match.group("type") + " " + (match.group("pointers") or "")).strip()
    return field_type, match.group("name")


def c_arg_type(arg: str) -> str:
    return c_decl_type_and_name(arg)[0]


def normalize_c_type(raw: str) -> str:
    array_match = re.match(r"(?P<type>.+)\[(?P<len>\d+)\]$", raw)
    if array_match:
        return f"[{normalize_c_type(array_match.group('type'))}; {array_match.group('len')}]"

    value = " ".join(raw.replace("*", " * ").split())
    pointer_count = value.count("*")
    is_const = value.startswith("const ")
    value = value.replace("const ", "").replace(" *", "").strip()
    base = {
        "void": "c_void",
        "char": "c_char",
        "int": "c_int",
        "size_t": "usize",
        "uint8_t": "u8",
        "uint32_t": "u32",
        "uint64_t": "u64",
        "int64_t": "i64",
        "float": "c_float",
        "double": "c_double",
    }.get(value, value)
    for index in range(pointer_count):
        mutability = "*const" if is_const and index == 0 else "*mut"
        base = f"{mutability} {base}"
    return base


def normalize_c_return_type(raw: str) -> str:
    if " ".join(raw.split()) == "void":
        return "()"
    return normalize_c_type(raw)


def normalize_rust_type(raw: str) -> str:
    value = " ".join(raw.split())
    value = value.replace("std::os::raw::", "")
    return value


def strip_typedefs(text: str) -> str:
    text = HEADER_ENUM.sub("", text)
    text = HEADER_STRUCT.sub("", text)
    text = HEADER_CALLBACK.sub("", text)
    return text


def compare_mapping(
    label: str,
    expected: dict[str, object],
    actual: dict[str, object],
    errors: list[str],
    allow_extra: bool = False,
) -> None:
    for name, expected_value in sorted(expected.items()):
        actual_value = actual.get(name)
        if actual_value is None:
            errors.append(f"missing Rust {label}: {name}")
        elif actual_value != expected_value:
            errors.append(f"{label} drift for {name}: header={expected_value} raw={actual_value}")

    extra = sorted(set(actual) - set(expected))
    if extra and not allow_extra:
        errors.append(f"extra Rust {label}s: {', '.join(extra)}")


def main() -> int:
    header = HEADER.read_text(encoding="utf-8")
    raw = RAW.read_text(encoding="utf-8")

    errors: list[str] = []
    compare_mapping("enum", enum_map(header, HEADER_ENUM), enum_map(raw, RUST_ENUM), errors)
    compare_mapping("struct", c_structs(header), rust_structs(raw), errors, allow_extra=True)
    compare_mapping("callback typedef", c_callbacks(header), rust_callbacks(raw), errors)
    compare_mapping("extern fn", c_functions(header), rust_functions(raw), errors)

    for error in errors:
        print(error)
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
