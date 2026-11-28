"""Counts, for each function of a C source tree, the places where control
branches (`if`, `case`, `?`, `&&`, `||`) and its loops, and prints the
largest functions. A crude count by braces, with comments and strings
removed; it is a size bound for the study, not a measurement of any
Whitefoot program.

usage: survey.py <label> <directory or file>..."""

import pathlib
import re
import sys

STRIP = re.compile(r'/\*.*?\*/|//[^\n]*|"(?:\\.|[^"\\\n])*"|\'(?:\\.|[^\'\\\n])*\'', re.S)
BRANCH = re.compile(r'\b(?:if|case)\b|\?|&&|\|\|')
LOOP = re.compile(r'\b(?:for|while|do)\b')
STATEMENT = re.compile(r';')
HEAD = re.compile(r'([A-Za-z_][A-Za-z_0-9]*)\s*\([^;{}]*\)\s*$')


def functions(text):
    text = STRIP.sub(" ", text)
    depth, start, name = 0, 0, None
    last = 0
    for match in re.finditer(r'[{}]', text):
        if match.group() == "{":
            if depth == 0:
                head = HEAD.search(text[last:match.start()].rstrip())
                name = head.group(1) if head else None
                start = match.end()
            depth += 1
        else:
            depth = max(depth - 1, 0)
            if depth == 0:
                if name and name not in ("if", "for", "while", "switch"):
                    yield name, text[start:match.start()]
                last = match.end()


def main():
    label = sys.argv[1]
    rows = []
    files = 0
    for root in sys.argv[2:]:
        path = pathlib.Path(root)
        paths = [path] if path.is_file() else list(path.rglob("*.c"))
        for source in paths:
            try:
                text = source.read_text(errors="replace")
            except OSError:
                continue
            files += 1
            for name, body in functions(text):
                rows.append((len(BRANCH.findall(body)), len(LOOP.findall(body)), len(STATEMENT.findall(body)), name, str(source)))
    rows.sort(reverse=True)
    counts = sorted(row[0] for row in rows)
    print(f"survey,{label},files,{files},functions,{len(rows)}")
    if counts:
        for share in (0.5, 0.9, 0.99, 0.999, 0.9999):
            print(f"quantile,{label},{share},{counts[min(len(counts) - 1, int(share * len(counts)))]}")
        for limit in (64, 256, 1024, 4096):
            print(f"above,{label},{limit},{sum(count > limit for count in counts)}")
    for branches, loops, statements, name, source in rows[:15]:
        print(f"largest,{label},{name},{branches},{loops},{statements},{source.split('/', 3)[-1]}")


if __name__ == "__main__":
    main()
