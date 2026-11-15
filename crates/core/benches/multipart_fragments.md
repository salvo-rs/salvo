# Multipart write fragmentation

`FilePart::create` coalesces small input fragments into a payload buffer capped
at 64 KiB before scheduling a blocking write. Large fragments are forwarded to
the blocking task directly; only the prefix needed to fill a pending buffer is
copied. A remaining tail is written in the existing final synchronization task.
Size limits, EOF validation, `sync_all`, and the writer-owned cleanup guard are
unchanged.

Run the manual release-mode probe with the workspace's normal build environment:

```powershell
cargo test -p salvo_core --release --test multipart_write_buffering fragmentation_parse_medians -- --ignored --nocapture
```

The fixture sends the same 16 KiB file in four fragment sizes. Each case has one
warmup and seven measured parses; request/fragment construction is outside the
timer. The timer includes parsing, temporary-file creation, writes and final
`sync_all`. The probe validates the resulting file size and prints median
nanoseconds. The ordinary regression test separately checks actual bytes across
buffer boundaries, including empty files and 1-byte/large fragments.

On Windows with Rust 1.99.1, the unchanged fixture was run on baseline
`e8128e21bd54ba737300f39754ab8160b607074a`, then after the buffering change in the
same workspace and release build environment:

| Input chunk bytes | Before median ns | After median ns |
|---:|---:|---:|
| 16,384 | 1,399,400 | 1,158,100 |
| 4,096 | 1,361,100 | 1,027,200 |
| 64 | 5,720,600 | 1,138,200 |
| 1 | 255,738,000 | 2,304,300 |

The 1-byte case demonstrates removal of a blocking-task round trip for each
fragment. These local results are not a production throughput claim. Real TCP
can coalesce fragments, HTTP/2 and HTTP/3 framing differ, and filesystem/cache
state affects these small synchronized uploads. Keep the manual probe ignored
in normal CI; correctness and cancellation/cleanup tests run without timing
thresholds.
