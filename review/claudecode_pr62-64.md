# ClaudeCode Review & Fix Plan (PR #62-64)

Context: sora-grayscale/mdp PRs #62, #63, #64. PR #64 includes #62 and #63 changes. This document is intended to be handed to ClaudeCode for fixes and follow-up changes.

## Scope

- PR #62: fix: Address code review issues (batch 1)
- PR #63: fix: Preserve heading attrs, fix mermaid safety, fix table headers
- PR #64: refactor: Stack-based parser with nested inline element support

## Top Priority Issues (Must Fix)

1) Terminal renderer loses parent styles on nested inline rendering
   - Problem: render_inline resets attributes inside nested elements. When a child element finishes, it resets and the parent style (bold/italic/underline) is lost.
   - Example:
     - Input: "**bold _italic_ more**"
     - Current: italic resets bold, "more" is no longer bold.
   - File: src/renderer/terminal.rs
   - Expected:
     - Bold should persist across children in a Strong block.
     - Link underline/color should remain across children inside a Link.
   - Fix strategy:
     - Introduce a simple attribute stack or re-apply parent styles after rendering child nodes.
     - Avoid calling Attribute::Reset in child nodes without restoring parent state.
     - Option: implement a small style state struct (bold/italic/strike/underline/color) and a render method that applies diff when entering/leaving nodes.

2) Inline parser termination is too aggressive
   - Problem: parse_inline_elements returns early on End(TagEnd::Item/Paragraph/BlockQuote/FootnoteDefinition). This can prematurely stop nested inline parsing when malformed input appears or when parsing link/strong content that spans boundaries.
   - File: src/parser.rs
   - Fix strategy:
     - Only terminate on explicit end_tag when provided.
     - For block-level end tags, exit only when end_tag is None and the caller expects it.
     - Ensure index handling does not skip next event after returning.

## High Priority Issues (Should Fix)

3) List items still cannot represent block content
   - Problem: ListItem is defined as { content: Vec<InlineElement>, sub_list: Option<Box<Element>> }.
   - This loses support for multiple paragraphs or block elements inside list items.
   - File: src/parser.rs
   - Fix strategy:
     - Redesign ListItem to hold Vec<Element> (block-level), not just inline.
     - This is a larger change; if not planned now, document limitation and add tests to track.

4) Link processing in HTML remains regex-based
   - Problem: process_links uses regex on HTML output. This fails with existing target attributes or different attribute ordering and can corrupt markup.
   - File: src/renderer/html.rs
   - Fix strategy:
     - Move link normalization into the markdown rendering stage if possible (pulldown-cmark event handling).
     - If not, use an HTML parser instead of regex.

## Medium Priority Issues

5) Mermaid encode/decode behavior has edge cases
   - Problem: Mermaid content is decode_html_entities then encode_text. Safe, but can affect intentionally encoded strings (e.g., &amp;).
   - File: src/renderer/html.rs
   - Fix strategy:
     - Add tests for mermaid code containing &amp; and < > characters.
     - Ensure output preserves intended literal characters.

6) Pager errors are still silent
   - Problem: child.wait() result is ignored; if pager crashes, no feedback.
   - File: src/main.rs
   - Fix strategy:
     - Add a warning on wait error or non-zero exit if appropriate (ignore normal quit).

7) File path normalization still accepts ".."
   - Problem: find_file normalizes ./ and path separators but does not block "..".
   - File: src/files.rs
   - Fix strategy:
     - Reject any path with ".." components for safety.
     - Or normalize and ensure resulting path stays within base_path.

## Tests to Add or Extend

- Terminal rendering for nested inline:
  - **bold _italic_ more** should keep bold across italic section.
  - [**bold link**](url) should keep underline/color and bold inside link.
- Mermaid edge cases:
  - Mermaid code containing &amp;, <, > should be preserved safely.
- Parser robustness:
  - Nested inline inside list items.
  - Mixed inline sequences around end tags.

## Notes

- PR #64 solved major parsing limitations by supporting nested inline elements.
- The renderer must follow through with proper style stacking, otherwise the new AST is not reflected correctly in terminal output.
- If list item block support is too big, document it clearly and plan a follow-up PR.
