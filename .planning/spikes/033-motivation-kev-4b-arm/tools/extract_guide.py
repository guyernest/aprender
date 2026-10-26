"""Extract the Kev instruction text from the partner's guide docx (Phase 12 plan 12-14, D-24 and D-11).

The instruction is guide Sections 4.1-4.4, 5 and 6, WITHOUT 4.5 (the derivation narrative; with it Sections 4-6 are 9,648
Qwen tokens, above Kev's 8,192-token row limit). The text is written ONLY under the gitignored /data/ root and this tool
prints only its sha256, word count and the section headings kept and dropped (never the text).

    extract_guide.py [--docx PATH] [--out data/kev/motivation-v1/guide-s4-6-no-4-5.txt] [--no-4-intro]
    extract_guide.py ... --drop-paragraphs-matching data/decide/motivation-v1/eval.jsonl
        D-11 removal branch: drops every guide paragraph (a table row counts as one) whose nfc-trim-ws-v1 normalized text
        contains a normalized eval post; rewrites the guide file; prints only the count. Needs scripts/laya_train/data.py
        (numpy), so run it with the kev vendor venv.

Standard library only for the extraction (zipfile + XML).
"""
import argparse, hashlib, importlib.util, json, re, sys, zipfile
import xml.etree.ElementTree as ET
from pathlib import Path

REPO = Path(__file__).resolve().parents[4]
W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
DOCX = "/Users/guy/projects/mcp/motivation-experiment/datasets/motivation/Classification_Guide_Motivational_Function_Iris_Yuster_25Oct2026.docx"
OUT = REPO / "data" / "kev" / "motivation-v1" / "guide-s4-6-no-4-5.txt"
ORDER = ["1", "2", "3", "4", "4.1", "4.2", "4.3", "4.4", "4.5", "5", "6", "7", "8", "9", "10", "10.1", "10.2", "10.3", "11"]
KEEP = ["4.1", "4.2", "4.3", "4.4", "5", "6"]


def text_of(el):
    return "".join(t.text or "" for t in el.iter(W + "t"))


def units(docx):
    """[(kind, text)] in document order: a paragraph is one unit, a table row is one unit (cells joined by ' | ')."""
    body = ET.fromstring(zipfile.ZipFile(docx).read("word/document.xml")).find(W + "body")
    out = []
    for el in body:
        tag = el.tag.replace(W, "")
        if tag == "p":
            out.append(("p", text_of(el)))
        elif tag == "tbl":
            for tr in el.iter(W + "tr"):
                cells = [" ".join(text_of(p) for p in tc.iter(W + "p")).strip() for tc in tr.findall(W + "tc")]
                out.append(("row", " | ".join(cells)))
    return out


def sectioned(us):
    """[(section_number | None, kind, text)]: a heading is the NEXT expected number from ORDER at the start of a short paragraph."""
    nxt, cur, out = 0, None, []
    for kind, txt in us:
        if kind == "p" and nxt < len(ORDER) and len(txt) < 110:
            num = ORDER[nxt]
            if re.match(r"^%s[.\s]" % re.escape(num), txt):
                cur = num; nxt += 1
                out.append((cur, "heading", txt))
                continue
        out.append((cur, kind, txt))
    return out


def build(docx, include_4_intro):
    sec = sectioned(units(docx))
    keep = set(KEEP) | ({"4"} if include_4_intro else set())
    kept = [(s, k, t) for s, k, t in sec if s in keep and t.strip()]
    kept_h = [s for s, k, t in sec if k == "heading" and s in keep]
    dropped_h = [s for s, k, t in sec if k == "heading" and s not in keep]
    return kept, kept_h, dropped_h


def render(kept):
    return "\n".join(t for _, _, t in kept) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--docx", default=DOCX); ap.add_argument("--out", default=str(OUT))
    ap.add_argument("--no-4-intro", dest="include_4_intro", action="store_false",
                    help="drop the Section 4 lead-in (heading 4 and its text before 4.1); default KEEPS it: that reading is the "
                         "research's measured 4,490 words / 6,811 Qwen tokens, without it the text is 6,469 tokens")
    ap.add_argument("--drop-paragraphs-matching", default=None, metavar="EVAL_JSONL")
    ap.add_argument("--expect-docx-sha256", default=None)
    a = ap.parse_args()
    raw = Path(a.docx).read_bytes()
    docx_sha = hashlib.sha256(raw).hexdigest()
    if a.expect_docx_sha256 is None:
        ev = json.load(open(REPO / ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-TASK-EVIDENCE.json"))
        a.expect_docx_sha256 = ev["guide_source"]["docx_sha256"]
    if docx_sha != a.expect_docx_sha256:
        sys.exit(f"REFUSED guide-sha: docx sha256 {docx_sha} != pinned {a.expect_docx_sha256}")
    kept, kept_h, dropped_h = build(a.docx, a.include_4_intro)
    removed = None
    if a.drop_paragraphs_matching:
        spec = importlib.util.spec_from_file_location("laya_data", REPO / "scripts" / "laya_train" / "data.py")
        data = importlib.util.module_from_spec(spec); spec.loader.exec_module(data)
        task = data.load_task(REPO / "data/decide/motivation-v1/task.json")
        posts = [data.normalize(t) for t, _ in data.load_rows(a.drop_paragraphs_matching, task, "eval")]
        before = len(kept)
        kept = [(s, k, t) for s, k, t in kept if k == "heading" or not any(p in data.normalize(t) for p in posts)]
        removed = before - len(kept)
    text = render(kept)
    out = Path(a.out); out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text, encoding="utf-8")
    print(json.dumps({"guide_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(), "words": len(text.split()),
                      "docx_sha256": docx_sha, "headings_kept": kept_h, "headings_dropped": dropped_h,
                      "section_4_lead_in_included": a.include_4_intro, "removed_paragraphs": removed, "out": str(out)}))


if __name__ == "__main__":
    main()
