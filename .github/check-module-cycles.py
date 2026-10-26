"""Reject runtime cycles between the crate's top-level modules."""

from pathlib import Path
import re
import sys

root = Path(sys.argv[1] if len(sys.argv) > 1 else ".") / "src"
nodes = {p.stem if p.is_file() else p.name for p in root.iterdir()}
nodes.discard("lib")
graph = {node: set() for node in nodes}


def code(text):
    # Ignore documentation, comments and literals, including braces in test fixtures.
    text = re.sub(
        r'r(?P<hashes>#{0,16})".*?"(?P=hashes)|"(?:\\.|[^"\\])*"|//[^\n]*|/\*.*?\*/',
        " ", text, flags=re.S,
    )
    # Code enabled by either a feature or tests also belongs to the runtime graph.
    while match := re.search(r"#\[cfg\((?:test|all\(\s*test\s*,[^]]*\))\)\]", text):
        start = match.start()
        item = re.search(r"[;{]", text[match.end():])
        assert item, "test configuration must decorate an item"
        end = match.end() + item.end()
        if item[0] == "{":
            depth = 1
            while depth:
                depth += (text[end] == "{") - (text[end] == "}")
                end += 1
        text = text[:start] + text[end:]
    return text


# Resolve the crate-root convenience exports as their owning modules.
aliases = {}
lib = code((root / "lib.rs").read_text())
for owner, single, group in re.findall(r"pub use (\w+)::(?:(\w+)|\{([^}]+)\});", lib):
    for name in (single or group).split(","):
        aliases[name.strip()] = owner

sources = {}
type_owners = {}
for path in sorted(root.rglob("*.rs")):
    relative = path.relative_to(root)
    owner = relative.parts[0] if len(relative.parts) > 1 else path.stem
    if owner == "lib":
        continue
    source = code(path.read_text())
    sources[path] = (owner, source)
    for name in re.findall(r"\b(?:struct|enum)\s+(\w+)", source):
        type_owners.setdefault(name, set()).add(owner)
    targets = re.findall(r"crate::(\w+)", source)
    for group in re.findall(r"crate::\{([^;]+)\};", source):
        depth = 0
        start = 0
        for index, char in enumerate(group + ","):
            depth += (char == "{") - (char == "}")
            if char == "," and depth == 0:
                item = group[start:index].strip().split("::")[0]
                targets.append(item.split(" as ")[0])
                start = index + 1
    graph[owner].update(
        target for name in targets
        if (target := aliases.get(name, name)) in nodes and target != owner
    )

# An inherent implementation must stay with its type when modules become crates.
for path, (owner, source) in sources.items():
    for signature in re.findall(r"\bimpl\b([^{}]*?)\{", source):
        if re.search(r"\bfor\b", signature):
            continue
        signature = signature.strip()
        if signature.startswith("<"):
            depth = 1
            end = 1
            while depth and end < len(signature):
                depth += (signature[end] == "<") - (signature[end] == ">")
                end += 1
            signature = signature[end:].strip()
        name = re.match(r"(?:\w+::)*(\w+)", signature)
        owners = type_owners.get(name[1], set()) if name else set()
        if len(owners) == 1 and owner not in owners:
            raise SystemExit(f"foreign inherent implementation: {path}: {name[1]} belongs to {next(iter(owners))}")

visited = set()


def visit(node, stack):
    if node in stack:
        raise SystemExit("module cycle: " + " -> ".join(stack[stack.index(node):] + [node]))
    if node in visited:
        return
    for target in sorted(graph[node]):
        visit(target, stack + [node])
    visited.add(node)


for node in sorted(graph):
    visit(node, [])
print("module graph: acyclic")
