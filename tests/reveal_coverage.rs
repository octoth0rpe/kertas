//! Revealed source styling must cover exactly its source, for every way a
//! block can be revealed (GPUI panics otherwise).

use kertas::definitions::Definitions;
use kertas::model::Model;
use kertas::reveal::styled_source;

fn check(text: &str) -> Vec<String> {
    let model = Model::new(text);
    let mut failures = Vec::new();
    for (index, item) in model.items.iter().enumerate() {
        for range in [model.source_range(index), item.range.clone()] {
            let source = &text[range.clone()];
            let styled = styled_source(source, item.kind, &model.definitions);
            let mut end = 0;
            let mut ok = true;
            for span in &styled.spans {
                ok &= span.range.start == end && span.range.end >= span.range.start;
                end = span.range.end;
            }
            ok &= end == source.len() || (source.is_empty() && styled.spans.is_empty());
            if !ok {
                failures.push(format!(
                    "item {index} {:?} {range:?}: {source:?} -> {:?}",
                    item.kind,
                    styled
                        .spans
                        .iter()
                        .map(|span| span.range.clone())
                        .collect::<Vec<_>>()
                ));
            }
        }
    }
    failures
}

#[test]
fn fixtures_are_covered() {
    for name in ["showcase.md", "10k.md"] {
        let text = std::fs::read_to_string(format!(
            "{}/spikes/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let failures = check(&text);
        assert!(
            failures.is_empty(),
            "{name}:\n{}",
            failures[..failures.len().min(5)].join("\n")
        );
    }
}

#[test]
fn edited_documents_are_covered() {
    for text in [
        "| a | b |\n|---|---|\n| c |\n\n[x]: /y\n",
        "Typed **bold** text\n\n[x]: /y",
        "# Smoke\n\nTyped **bo",
        "- a\n- [ ] b\n\n> q\\\n\n\n",
        "\n\n# Heading\n\n\n",
        "```\ncode",
    ] {
        let failures = check(text);
        assert!(failures.is_empty(), "{text:?}:\n{}", failures.join("\n"));
    }
}

/// A list whose indented code block's styling once ran past the end of the
/// source (found by the editing smoke test; GPUI aborted on it).
#[test]
fn list_with_indented_code_is_covered() {
    let source = include_str!("fixtures/list-with-indented-code.md");
    let styled = styled_source(
        source,
        kertas::blocks::BlockKind::List { ordered: false },
        &Definitions::from_text("[docs]: https://example.com/docs"),
    );
    let end = styled.spans.last().map_or(0, |span| span.range.end);
    assert_eq!(end, source.len());
}
