# Static inability text without transient allocations

R-EW4 concerns owned diagnostic refusal messages created after a failure verdict,
then discarded. Preserve the existing semantic NoVerdict/Failure partition. The
independent reviewer approved the following greenfield representation instead of
introducing a parallel private semantic error hierarchy or a guessed reserve.

NoVerdict code/message and internal-schema Failure code/message use
`Cow<'static, str>`. Their constructors accept `Into<Cow<'static, str>>` so static
SDK strings stay borrowed throughout construction and cloning. Dynamic owned
strings move into records without another allocation; callers with nonstatic
borrowed text explicitly make an owned value. Serialized code/message strings,
typed reasons, redacted Debug behavior and lifetime independence remain intact.
These Rust field carriers have no compatibility obligation.

The original interpreter's mapping of SDK Attempt LimitExceeded/Cancelled must
move code text directly to NoVerdict and borrow its fixed explanatory message.
Do not pass these through PreparationFailure or `into_owned`, since those
intermediate owned carriers recreate the diagnosed allocation. Preserve the
current no-origin policy for control refusals; other preparation failures retain
their existing semantic mapping. Arbitrary callback-owned allocations are outside
this bounded SDK construction claim.

Audit helper signatures and conversions, not only the final constructors. Literal
borrowing must survive end-to-end; owned strings must move rather than clone.
Existing source URI/pointer/cause accounting remains necessary. R-EW2's Fails
message reserve remains: that public result currently owns its message String.
This change does not claim all preparation-result construction is allocation free.

Validation: static cold/warm construction and cloning request zero bytes; owned
String move/drop and exact serialization; SDK Attempt work/storage/cancellation
conversion; all long-URI post-verdict cancellation/refusal cases; unchanged 42
Explain cases and forced-original required corpus; current/minimum Rust, native
stack, actual Node/browser hosts and complete portable source checks. Reconcile
public Rust reference output with the new actual field types. Preserve the R-EW4
counterexample and demonstrate zero retained ownership after every refusal.
