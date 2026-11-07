# Locked HIR translator error/source-ownership closure

Scope: regex-syntax 0.8.11, a successful AST parse of the SDK normalized guard's closed grammar, a fresh TranslatorBuilder with explicit utf8(true) and otherwise its reviewed defaults. The complete translator source and guard hashes are pinned in source-hashes.json. This certificate does not cover general Rust regex syntax, arbitrary AST construction, or a reused translator with unrelated state.

The unique translator error constructor (`translate.rs`, `TranslatorI::error`, line 793 at this pin) executes `self.pattern.to_string()`. All calls are excluded as follows:

| Error-producing path | Required condition | Why it cannot occur here |
|---|---|---|
| ast_literal_to_scalar / InvalidUtf8 | Unicode off and a non-ASCII byte literal | All literals/classes use default Unicode. The only Unicode-off group is the atomic exact `(?-u:\b)` token containing no literal/class. Group exit restores prior flags. |
| case_fold_char, unicode_fold_and_negate, binary class folding / UnicodeCaseUnavailable | Case-insensitive flag | The builder defaults it off; the guard excludes case flags and directives. |
| hir_dot / InvalidUtf8 or InvalidLineTerminator | Dot AST and exceptional mode/terminator | The guard excludes dot; Reader lowers it to scalar ranges. Defaults are also fixed. |
| hir_unicode_class / UnicodeNotAllowed and property lookup errors | Unexpanded Unicode-property AST | The guard excludes property escapes. Reader resolves supported properties into explicit scalar classes before this stage. |
| hir_perl_unicode_class / property lookup errors | Perl class AST | The guard excludes Perl classes; Reader lowers them. |
| hir_perl_byte_class, bytes_fold_and_negate / InvalidUtf8 | Byte-oriented class | No class occurs within the only Unicode-off token. |
| class_literal_byte / UnicodeNotAllowed | A non-ASCII literal inside a byte class | Same exclusion. |
| visit_class_set_binary_op_post / case-fold errors | Nested class set operation plus case mode | Guard allows only ordered flat positive scalar ranges/literals; no nested algebra or case mode. |

Assertions, repetition, concatenation, alternation, and unnamed capture translation introduce no Result error path of their own. Guard-admitted assertions are absolute ^/$ and the fixed ASCII boundary; default multiline/swap-greed/CRLF flags are unchanged. Guard-admitted capture groups have indices, not names, so translation does not clone capture-name strings. Checked parser representation failures occur before this stage and remain covered by the AST schedule.

Thus no correct guarded translation reaches the source-owning HIR Error constructor. Keeping the borrowed form argument still provides correct source identity and future defensive behavior. An unexpected returned error is an internal/representation refusal, dropped without formatting and never exposed as authored invalidity or Undefined. Adding new flags/tokens or changing the locked translator requires re-auditing every error site before relying on the exclusion.

Finite corroboration: the configuration probe's 1,175 successful guarded translations reached no error under either UTF8 setting and had identical HIR. Six configured-program groups also preserve independently fixed existence answers. Neither observation replaces the source closure.
