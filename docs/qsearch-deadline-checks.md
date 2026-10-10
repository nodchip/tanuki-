# Deadline checks during quiescence search

Quiescence search can spend a long time in a tactical subtree without returning
to main search. Checking deadlines only in main search therefore does not bound
that subtree. The repair calls the existing `SearchManager::check_time` from the
main-thread quiescence search; it does not introduce a second timer policy or
increase the match runner's allowed time.

After node/PV initialization, all search threads observe the existing stop flag.
After recursive search, the move is undone before testing the stop flag again.
An interrupted result returns `VALUE_ZERO` without updating the best value, PV,
or transposition table, matching main search's cancellation handling.

The existing check throttle, pondering behavior, and completed-depth gate remain
unchanged. In particular, this is not a guarantee that the first iteration can
always finish within a requested millisecond budget. A stop may also occur between
checks. Normal native exit is not evidence that a match had no timing violations.

## Focused validation

Use the existing engine commands with the same network, options, compiler and CPU
target in separate unmodified and patched builds. Keep outputs separate and
inspect the actual results, not just the native process exit code.

- `WarmHistoryMovetime_All51SearchesWithinExistingMargin`: native
  `bench 128 1 200 <position-history-file> movetime`; inspect every position,
  bestmove and reported time. Compare with an unmodified matched build. A passing
  control means that this run does not establish a timing improvement.
- `FixedNodes_FiniteLegalSearchAndNextSearch`: native
  `bench 128 1 20000 <position-history-file> nodes`; inspect all search counts and
  node budgets, followed by the existing `unittest random_player_loop 0` suite.
  The finite node test is not a substitute for time-control qualification.
- `ExternalStop_RestoresPositionAndAllowsNextSearch` and
  `TwoThreads_StopPropagatesAndNormalExit`: after `isready`, request two threads,
  use `position startpos`, start `go movetime 5000`, and send one `stop` after the
  first completed-depth info. Verify a legal bestmove, use `d` to check the board
  is unchanged, run another `go nodes 20000` without replacing the position,
  and verify the second bestmove and board. End with `quit` and observe normal
  exit and no surviving engine threads/processes.

The stop lifecycle test does not prove where inside search the stop arrived.
Review the cancellation guards as well. Do not publish private position/history
fixtures, networks, PVs, or runtime logs with this source repair. Separate finite
concurrent time-control qualification and strength testing are required before
using a changed engine in production or adopting it as a champion.
