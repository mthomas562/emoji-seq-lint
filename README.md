# emoji-seq-lint

A validator for `.seq` files: plain-text definitions of named emoji
sequences, the kind of data file that ends up backing a custom emoji
picker, a chat client's shortcode registry, or an icon build step.

```
:flag_us: 1F1FA 1F1F8
:family_mwgb: 1F468 200D 1F469 200D 1F466 200D 1F466
:thumbsup_tone2: 1F44D 1F3FC
```

These files are edited by hand, and the failure mode is quiet: a
dropped `200D` in a ZWJ chain, a typo'd hex digit, one skin tone
modifier too many, and the entry silently renders as tofu or two
separate glyphs instead of the sequence you meant. Nothing catches
that until someone notices it looks wrong in the UI. This tool parses
the file and checks that each entry is a structurally valid emoji
sequence (single codepoint, ZWJ sequence, regional-indicator flag
pair, skin tone modifier pair, keycap, or tag sequence), and reports
every problem with the line and column of the exact token at fault.

## Format

One entry per line: a `:name:` (lowercase letters, digits, `_`, `-`,
`+`), whitespace, then one or more codepoints in hex, optionally
prefixed with `U+`. Blank lines and lines starting with `#` are
ignored.

```
# comment
:keycap_5: 35 FE0F 20E3
:tag_england: 1F3F4 E0067 E0062 E0065 E006E E0067 E007F
```

## CLI usage

```
$ emojiseq check emoji.seq
emoji.seq: 4 entries, no errors
```

When something is wrong, the output points at it directly:

```
$ emojiseq check broken.seq
error: two consecutive zero-width joiners
 --> broken.seq:2:29
  |
2 | :family_mwgb: 1F468 200D 200D 1F469
  |                            ^^^

error: "1F68X" is not a valid hexadecimal codepoint
 --> broken.seq:5:11
  |
5 | :train: 1F68X
  |          ^^^^^

2 errors in broken.seq
```

## Library usage

```rust
use emoji_seq_lint::{classify, parse};

let source = ":flag_us: 1F1FA 1F1F8\n";
let (entries, diagnostics) = parse(source);

for diag in &diagnostics {
    eprint!("{}", diag.render("emoji.seq", source));
}

for entry in &entries {
    match classify(entry) {
        Ok(kind) => println!("{}: {:?}", entry.name, kind),
        Err(diag) => eprint!("{}", diag.render("emoji.seq", source)),
    }
}
```

## Status

Early skeleton. The parser and structural classifier work; what's
missing is cross-referencing entries against the actual Unicode
recommended-for-general-interchange sequence list, so right now a
sequence can be structurally well-formed (right shape of ZWJ joins,
right kind of modifier) without being a sequence any font or platform
actually renders. See the roadmap for what's next.

No third-party dependencies. Standard library only.
