# Allocation/cancellation evidence chronology

The first source read had direct try_reserve_exact failure→Size mapping. By the
time the compiled allocator witness was built, the parent had already added the
completion check. Consequently `allocation-stop-before.log` and the unfortunately
named `before-fix-nfa.rs` are **fixed-source positive evidence**, not original
failing evidence. Their actual source hash is retained.

The negative control in `reconstructed-old-reservation.rs` changes only that one
branch back to the code observed in the earlier tool read. Its injected allocation
failure returns Size even when the allocator simultaneously sets cancellation.
The current production branch returns Attempt(Cancelled); without cancellation
both return local Size. The negative is explicitly a reconstructed source control,
not a claim that an old binary was retained. No production file was edited.
