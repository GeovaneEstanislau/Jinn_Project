from pathlib import Path
import re

root = Path('docs')
issues = []
pattern = re.compile(r'\[([^\]]+)\]\(([^\)]+\.md)\)')
for p in sorted(root.rglob('*.md')):
    try:
        text = p.read_text(encoding='utf-8')
    except UnicodeDecodeError:
        text = p.read_text(encoding='latin-1')
    for m in pattern.finditer(text):
        target = (p.parent / m.group(2)).resolve()
        if not target.exists():
            issues.append((str(p), m.group(0), m.group(2), str(target)))

if issues:
    print('Broken links found:')
    for file, link, rel, target in issues:
        print(f'- {file}: {link} -> {rel} (resolved {target})')
else:
    print('No broken markdown links found in docs.')
