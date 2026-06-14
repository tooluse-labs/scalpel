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
HEADER = ROOT / "crates/scalpel-shim/include/pdbg_shim.h"
RAW = ROOT / "crates/scalpel-shim/src/raw.rs"

HEADER_ENUM = re.compile(r"typedef\s+enum\s+(\w+)\s*\{(?P<body>.*?)\}\s*\1\s*;", re.S)
RUST_ENUM = re.compile(r"pub\s+enum\s+(\w+)\s*\{(?P<body>.*?)\}", re.S)
VARIANT_ITEM = re.compile(r"^(?P<name>[A-Z][A-Z0-9_]+)(?:\s*=\s*(?P<value>[0-9]+))?$")
HEADER_STRUCT = re.compile(r"typedef\s+struct\s+(\w+)\s*\{(?P<body>.*?)\}\s*\1\s*;", re.S)
HEADER_OPAQUE_STRUCT = re.compile(r"typedef\s+struct\s+(\w+)\s+\1\s*;")
RUST_STRUCT = re.compile(r"pub\s+struct\s+(\w+)\s*\{(?P<body>.*?)\n\}", re.S)
RUST_REPR_ITEM = re.compile(
    r"(?P<attrs>(?:#\[[^\]]+\]\s*)*)pub\s+(?P<kind>struct|enum)\s+(?P<name>\w+)"
)
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
        out[name] = enum_variants(name, body)
    return out


def enum_variants(enum_name: str, body: str) -> dict[str, int]:
    variants: dict[str, int] = {}
    next_value = 0
    for raw_item in body.split(","):
        item = strip_enum_item_comments(raw_item).strip()
        if not item:
            continue
        variant_match = VARIANT_ITEM.match(item)
        if not variant_match:
            raise ValueError(f"cannot parse enum variant in {enum_name}: {item}")
        value = variant_match.group("value")
        if value is not None:
            next_value = int(value)
        variants[variant_match.group("name")] = next_value
        next_value += 1
    return variants


def strip_enum_item_comments(item: str) -> str:
    item = re.sub(r"/\*.*?\*/", "", item, flags=re.S)
    return re.sub(r"//.*", "", item)


def c_structs(text: str) -> dict[str, list[tuple[str, str]]]:
    structs: dict[str, list[tuple[str, str]]] = {}
    for match in HEADER_STRUCT.finditer(text):
        fields: list[tuple[str, str]] = []
        for statement in statements(match.group("body")):
            field_type, field_name = c_decl_type_and_name(statement)
            fields.append((field_name, normalize_c_type(field_type)))
        structs[match.group(1)] = fields
    return structs


def c_opaque_structs(text: str) -> set[str]:
    return set(HEADER_OPAQUE_STRUCT.findall(text))


def rust_structs(text: str) -> dict[str, list[tuple[str, str]]]:
    structs: dict[str, list[tuple[str, str]]] = {}
    for match in RUST_STRUCT.finditer(text):
        fields: list[tuple[str, str]] = []
        for line in match.group("body").splitlines():
            line = line.strip().rstrip(",")
            if not line or line.startswith("#") or ":" not in line:
                continue
            name, raw_type = line.removeprefix("pub ").split(":", 1)
            fields.append((name.strip(), normalize_rust_type(raw_type.strip())))
        structs[match.group(1)] = fields
    return structs


def rust_repr_c_items(text: str) -> set[str]:
    items: set[str] = set()
    for match in RUST_REPR_ITEM.finditer(text):
        if "#[repr(C)]" in match.group("attrs"):
            items.add(match.group("name"))
    return items


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
    body = re.sub(r"/\*.*?\*/", "", body, flags=re.S)
    body = re.sub(r"//.*", "", body)
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
    header_enums = enum_map(header, HEADER_ENUM)
    header_structs = c_structs(header)
    header_opaque_structs = c_opaque_structs(header)
    rust_struct_map = rust_structs(raw)
    rust_concrete_structs = {
        name: fields
        for name, fields in rust_struct_map.items()
        if name not in header_opaque_structs
    }
    expected_opaque_structs = {
        name: [("_private", "[u8; 0]")] for name in header_opaque_structs
    }

    compare_mapping("enum", header_enums, enum_map(raw, RUST_ENUM), errors)
    compare_mapping("struct", header_structs, rust_concrete_structs, errors)
    compare_mapping(
        "opaque struct",
        expected_opaque_structs,
        {name: rust_struct_map[name] for name in header_opaque_structs if name in rust_struct_map},
        errors,
    )
    compare_mapping("callback typedef", c_callbacks(header), rust_callbacks(raw), errors)
    compare_mapping("extern fn", c_functions(header), rust_functions(raw), errors)
    expected_repr_c = set(header_enums) | set(header_structs) | header_opaque_structs
    missing_repr_c = sorted(expected_repr_c - rust_repr_c_items(raw))
    if missing_repr_c:
        errors.append(f"missing #[repr(C)] on Rust ABI items: {', '.join(missing_repr_c)}")

    for error in errors:
        print(error)
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
