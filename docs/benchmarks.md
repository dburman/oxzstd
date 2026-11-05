# Benchmarks

oxzstd against the reference C `zstd`, both single-threaded.

**Machine:** an Apple silicon (M1-class, arm64) Mac, one core per run. **Builds:** Rust
1.99.1, release profile (fat LTO, one codegen unit); `zstd` 1.5.7.

**Inputs:** `words` is the system word list (`/usr/share/dict/words`); the rest come from
the test corpus (`tests/common/mod.rs`), generated from fixed seeds: `text-2m` is
English-like text, `html-500k` HTML, `skewed-300k` three symbols with skewed frequencies,
`random-100k` random bytes.

**Speed and size:** `cargo bench --bench versus_zstd -- LEVELS [NAMES]`. Files in
`bench-data/` are added to the inputs (copy `words` there). oxzstd is timed through
`compress_with` and `decompress_into` (into a reused buffer), the best of repeated runs over
about half a second; the reference through `zstd -b`. Neither side's frames carry a checksum
(the reference's benchmark leaves it out; oxzstd's `Options::checksum(false)`). Verifying one
costs oxzstd about 5% of the decoding time for compressible inputs.

**Memory:** `scripts/bench-memory.sh FILE...`: peak resident memory of the whole process,
from `/usr/bin/time -l`. For oxzstd, `examples/compress` and `examples/decompress`, which
hold the whole input and output; for the reference, `zstd FILE -o OUT` and
`zstd -d FILE -o OUT`, which stream. A process that does nothing measures 1.0 MiB.

## Speed, size and memory

### words

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd | Compress RAM MiB | Decompress RAM MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | -0.49% | 504 / 591 | 85% | 1170 / 1424 | 82% | 7.9 / 9.3 | 5.8 / 5.8 |
| -1 | -0.01% | 403 / 474 | 85% | 967 / 1157 | 84% | 7.7 / 9.0 | 5.5 / 5.5 |
| 1 | +0.04% | 396 / 476 | 83% | 867 / 1088 | 80% | 6.3 / 8.6 | 5.4 / 5.3 |
| 3 | +0.10% | 238 / 237 | 100% | 724 / 884 | 82% | 7.1 / 8.9 | 5.3 / 6.8 |
| 5 | +0.46% | 140 / 129 | 109% | 636 / 786 | 81% | 9.4 / 10.6 | 5.2 / 6.6 |
| 7 | +0.22% | 69 / 77 | 90% | 671 / 836 | 80% | 11.3 / 12.9 | 5.3 / 6.6 |
| 9 | +0.23% | 51 / 58 | 88% | 683 / 854 | 80% | 16.3 / 17.9 | 5.2 / 6.7 |
| 12 | +0.20% | 29 / 30 | 97% | 689 / 889 | 78% | 46.3 / 47.8 | 5.2 / 6.7 |
| 16 | -0.34% | 12 / 10 | 120% | 560 / 749 | 75% | 38.4 / 39.8 | 5.2 / 6.7 |
| 19 | +0.09% | 4 / 5 | 80% | 529 / 712 | 74% | 55.2 / 56.3 | 5.2 / 6.7 |
| 22 | +0.04% | 3 / 3 | 100% | 524 / 734 | 71% | 71.6 / 72.3 | 5.2 / 6.7 |

### html-500k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd | Compress RAM MiB | Decompress RAM MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | -0.30% | 660 / 625 | 106% | 1760 / 1775 | 99% | 3.5 / 3.5 | 3.1 / 3.3 |
| -1 | -0.04% | 606 / 566 | 107% | 1592 / 1661 | 96% | 3.5 / 3.5 | 2.5 / 3.3 |
| 1 | -0.02% | 573 / 542 | 106% | 1494 / 1669 | 90% | 3.5 / 3.5 | 2.5 / 3.3 |
| 3 | +0.08% | 474 / 464 | 102% | 1550 / 1623 | 96% | 4.2 / 4.2 | 2.5 / 3.3 |
| 5 | +0.16% | 230 / 199 | 116% | 1491 / 1562 | 95% | 6.5 / 5.9 | 2.5 / 3.3 |
| 7 | -0.03% | 109 / 116 | 94% | 1639 / 1689 | 97% | 8.5 / 8.4 | 2.5 / 3.3 |
| 9 | +0.01% | 79 / 83 | 95% | 1664 / 1746 | 95% | 8.4 / 8.4 | 2.5 / 3.3 |
| 12 | +0.02% | 48 / 47 | 102% | 1716 / 1848 | 93% | 8.5 / 8.4 | 2.5 / 3.3 |
| 16 | +0.06% | 9 / 9 | 100% | 1672 / 1684 | 99% | 11.3 / 11.5 | 2.5 / 3.3 |
| 19 | +0.05% | 4 / 4 | 100% | 1582 / 1637 | 97% | 12.8 / 12.1 | 2.5 / 3.3 |
| 22 | +0.05% | 4 / 5 | 80% | 1578 / 1644 | 96% | 12.6 / 12.1 | 2.5 / 3.3 |

### text-2m

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd | Compress RAM MiB | Decompress RAM MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | -0.14% | 412 / 379 | 109% | 1180 / 1114 | 106% | 5.7 / 7.5 | 6.6 / 5.0 |
| -1 | -0.04% | 390 / 363 | 107% | 1109 / 1079 | 103% | 5.9 / 7.4 | 6.5 / 5.1 |
| 1 | -0.03% | 380 / 352 | 108% | 1170 / 1183 | 99% | 5.8 / 7.1 | 6.4 / 4.9 |
| 3 | +0.07% | 332 / 314 | 106% | 1253 / 1196 | 105% | 6.3 / 7.7 | 6.4 / 6.1 |
| 5 | -0.11% | 145 / 130 | 112% | 1085 / 1034 | 105% | 8.5 / 9.5 | 6.4 / 6.1 |
| 7 | -0.01% | 61 / 73 | 84% | 1242 / 1224 | 101% | 10.5 / 12.0 | 6.4 / 6.0 |
| 9 | -0.03% | 43 / 53 | 81% | 1286 / 1291 | 100% | 15.5 / 17.0 | 6.4 / 6.0 |
| 12 | -0.04% | 22 / 23 | 96% | 1424 / 1461 | 97% | 25.4 / 26.9 | 6.3 / 6.1 |
| 16 | +0.06% | 4 / 5 | 80% | 1450 / 1451 | 100% | 36.8 / 38.9 | 6.3 / 5.9 |
| 19 | +0.03% | 3 / 3 | 100% | 1446 / 1466 | 99% | 38.8 / 39.4 | 6.3 / 5.9 |
| 22 | +0.03% | 3 / 4 | 75% | 1446 / 1466 | 99% | 38.6 / 39.4 | 6.3 / 5.9 |

## Speed and size, every input

### words (2,493,885 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 1,428,912 | 1,435,949 | -0.49% | 504 / 591 | 85% | 1170 / 1424 | 82% |
| -1 | 1,127,878 | 1,127,996 | -0.01% | 403 / 474 | 85% | 967 / 1157 | 84% |
| 1 | 890,162 | 889,777 | +0.04% | 396 / 476 | 83% | 867 / 1088 | 80% |
| 2 | 863,682 | 863,511 | +0.02% | 306 / 294 | 104% | 779 / 964 | 81% |
| 3 | 853,674 | 852,806 | +0.10% | 238 / 237 | 100% | 724 / 884 | 82% |
| 4 | 855,293 | 854,288 | +0.12% | 238 / 236 | 101% | 719 / 870 | 83% |
| 5 | 802,409 | 798,716 | +0.46% | 140 / 129 | 109% | 636 / 786 | 81% |
| 6 | 770,771 | 767,493 | +0.43% | 92 / 86 | 107% | 663 / 816 | 81% |
| 7 | 770,232 | 768,540 | +0.22% | 69 / 77 | 90% | 671 / 836 | 80% |
| 8 | 749,333 | 747,610 | +0.23% | 53 / 57 | 93% | 680 / 874 | 78% |
| 9 | 749,924 | 748,233 | +0.23% | 51 / 58 | 88% | 683 / 854 | 80% |
| 12 | 752,261 | 750,736 | +0.20% | 29 / 30 | 97% | 689 / 889 | 78% |
| 16 | 674,893 | 677,167 | -0.34% | 12 / 10 | 120% | 560 / 749 | 75% |
| 19 | 662,516 | 661,923 | +0.09% | 4 / 5 | 80% | 529 / 712 | 74% |
| 22 | 662,135 | 661,855 | +0.04% | 3 / 3 | 100% | 524 / 734 | 71% |

### text-2m (2,000,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 740,681 | 741,684 | -0.14% | 412 / 379 | 109% | 1180 / 1114 | 106% |
| -1 | 670,645 | 670,894 | -0.04% | 390 / 363 | 107% | 1109 / 1079 | 103% |
| 1 | 601,675 | 601,830 | -0.03% | 380 / 352 | 108% | 1170 / 1183 | 99% |
| 2 | 602,682 | 602,850 | -0.03% | 380 / 358 | 106% | 1046 / 1024 | 102% |
| 3 | 569,546 | 569,150 | +0.07% | 332 / 314 | 106% | 1253 / 1196 | 105% |
| 4 | 566,181 | 565,958 | +0.04% | 336 / 323 | 104% | 1268 / 1193 | 106% |
| 5 | 572,858 | 573,488 | -0.11% | 145 / 130 | 112% | 1085 / 1034 | 105% |
| 6 | 559,758 | 560,444 | -0.12% | 91 / 81 | 112% | 1173 / 1142 | 103% |
| 7 | 550,766 | 550,823 | -0.01% | 61 / 73 | 84% | 1242 / 1224 | 101% |
| 8 | 549,589 | 549,737 | -0.03% | 44 / 53 | 83% | 1285 / 1269 | 101% |
| 9 | 549,435 | 549,586 | -0.03% | 43 / 53 | 81% | 1286 / 1291 | 100% |
| 12 | 523,830 | 524,029 | -0.04% | 22 / 23 | 96% | 1424 / 1461 | 97% |
| 16 | 462,173 | 461,886 | +0.06% | 4 / 5 | 80% | 1450 / 1451 | 100% |
| 19 | 459,178 | 459,028 | +0.03% | 3 / 3 | 100% | 1446 / 1466 | 99% |
| 22 | 459,178 | 459,028 | +0.03% | 3 / 4 | 75% | 1446 / 1466 | 99% |

### html-500k (500,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 132,277 | 132,678 | -0.30% | 660 / 625 | 106% | 1760 / 1775 | 99% |
| -1 | 118,925 | 118,970 | -0.04% | 606 / 566 | 107% | 1592 / 1661 | 96% |
| 1 | 105,954 | 105,973 | -0.02% | 573 / 542 | 106% | 1494 / 1669 | 90% |
| 2 | 103,825 | 103,845 | -0.02% | 543 / 514 | 106% | 1398 / 1506 | 93% |
| 3 | 98,779 | 98,697 | +0.08% | 474 / 464 | 102% | 1550 / 1623 | 96% |
| 4 | 98,610 | 98,535 | +0.08% | 473 / 469 | 101% | 1544 / 1633 | 95% |
| 5 | 93,700 | 93,549 | +0.16% | 230 / 199 | 116% | 1491 / 1562 | 95% |
| 6 | 91,615 | 91,705 | -0.10% | 151 / 136 | 111% | 1602 / 1643 | 98% |
| 7 | 90,558 | 90,585 | -0.03% | 109 / 116 | 94% | 1639 / 1689 | 97% |
| 8 | 90,903 | 90,892 | +0.01% | 79 / 87 | 91% | 1658 / 1741 | 95% |
| 9 | 90,903 | 90,892 | +0.01% | 79 / 83 | 95% | 1664 / 1746 | 95% |
| 12 | 89,261 | 89,240 | +0.02% | 48 / 47 | 102% | 1716 / 1848 | 93% |
| 16 | 82,840 | 82,794 | +0.06% | 9 / 9 | 100% | 1672 / 1684 | 99% |
| 19 | 82,352 | 82,310 | +0.05% | 4 / 4 | 100% | 1582 / 1637 | 97% |
| 22 | 82,352 | 82,310 | +0.05% | 4 / 5 | 80% | 1578 / 1644 | 96% |

### skewed-300k (300,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 100,492 | 100,313 | +0.18% | 444 / 405 | 110% | 870 / 923 | 94% |
| -1 | 90,734 | 90,545 | +0.21% | 414 / 385 | 108% | 808 / 865 | 93% |
| 1 | 80,711 | 80,700 | +0.01% | 422 / 392 | 108% | 934 / 985 | 95% |
| 2 | 82,911 | 82,824 | +0.11% | 389 / 359 | 108% | 782 / 846 | 92% |
| 3 | 80,194 | 80,152 | +0.05% | 359 / 349 | 103% | 1013 / 1043 | 97% |
| 4 | 80,088 | 80,114 | -0.03% | 355 / 348 | 102% | 1011 / 1042 | 97% |
| 5 | 80,422 | 80,404 | +0.02% | 152 / 126 | 121% | 859 / 920 | 93% |
| 6 | 76,713 | 76,715 | -0.00% | 89 / 74 | 120% | 981 / 1046 | 94% |
| 7 | 75,840 | 75,847 | -0.01% | 57 / 64 | 89% | 1083 / 1130 | 96% |
| 8 | 75,029 | 75,026 | +0.00% | 42 / 46 | 91% | 1116 / 1176 | 95% |
| 9 | 75,029 | 75,026 | +0.00% | 42 / 46 | 91% | 1116 / 1184 | 94% |
| 12 | 73,414 | 73,409 | +0.01% | 21 / 20 | 105% | 1279 / 1348 | 95% |
| 16 | 61,686 | 61,685 | +0.00% | 5 / 5 | 100% | 1402 / 1951 | 72% |
| 19 | 64,315 | 64,315 | +0.00% | 2 / 3 | 67% | 1401 / 1685 | 83% |
| 22 | 64,315 | 64,315 | +0.00% | 2 / 3 | 67% | 1396 / 1678 | 83% |

### random-100k (100,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 100,012 | 100,012 | +0.00% | 14908 / 15072 | 99% | 68587 / 43563 | 157% |
| -1 | 100,012 | 100,012 | +0.00% | 14035 / 13685 | 103% | 68587 / 59258 | 116% |
| 1 | 100,012 | 100,012 | +0.00% | 8989 / 9679 | 93% | 68587 / 58519 | 117% |
| 2 | 100,012 | 100,012 | +0.00% | 8391 / 9308 | 90% | 68587 / 52752 | 130% |
| 3 | 100,012 | 100,012 | +0.00% | 5970 / 7433 | 80% | 68587 / 47243 | 145% |
| 4 | 100,012 | 100,012 | +0.00% | 4888 / 7353 | 66% | 68587 / 57666 | 119% |
| 5 | 100,012 | 100,012 | +0.00% | 4054 / 4218 | 96% | 68587 / 60211 | 114% |
| 6 | 100,012 | 100,012 | +0.00% | 4047 / 4237 | 96% | 68587 / 59790 | 115% |
| 7 | 100,012 | 100,012 | +0.00% | 4096 / 3829 | 107% | 68587 / 60986 | 112% |
| 8 | 100,012 | 100,012 | +0.00% | 3774 / 4231 | 89% | 68587 / 38385 | 179% |
| 9 | 100,012 | 100,012 | +0.00% | 3458 / 3779 | 92% | 68587 / 37141 | 185% |
| 12 | 100,012 | 100,012 | +0.00% | 741 / 754 | 98% | 68587 / 32090 | 214% |
| 16 | 100,012 | 100,012 | +0.00% | 53 / 45 | 118% | 68587 / 35958 | 191% |
| 19 | 100,012 | 100,012 | +0.00% | 27 / 23 | 117% | 96061 / 42424 | 226% |
| 22 | 100,012 | 100,012 | +0.00% | 26 / 22 | 118% | 96061 / 30545 | 314% |

## x86_64

The tables above are on Apple silicon. These are on an x86_64 Linux virtual machine (QEMU,
AVX2 and BMI2), one core per run, in a Debian container. The reference is Debian's `zstd`
1.5.7; oxzstd is built with Rust 1.99.1 for the default x86-64 target, with its own release
profile. Same benchmark, same method; `html-4k` and `html-16k` are the benchmark's small
inputs, the size of most web responses.

### html-4k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | +0.00% | 485 / 490 | 99% | 1026 / 1195 | 86% |
| 1 | -0.10% | 355 / 349 | 102% | 688 / 813 | 85% |
| 3 | +0.10% | 297 / 295 | 101% | 655 / 784 | 84% |
| 5 | +0.00% | 161 / 159 | 101% | 677 / 796 | 85% |
| 9 | +0.31% | 78 / 66 | 118% | 681 / 815 | 84% |
| 12 | +0.00% | 17 / 18 | 94% | 673 / 809 | 83% |
| 19 | +0.21% | 3 / 3 | 100% | 668 / 789 | 85% |

### html-16k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | +0.00% | 545 / 487 | 112% | 1066 / 1206 | 88% |
| 1 | +0.08% | 463 / 411 | 113% | 880 / 1033 | 85% |
| 3 | +0.17% | 372 / 346 | 108% | 842 / 975 | 86% |
| 5 | +0.09% | 132 / 132 | 100% | 896 / 1017 | 88% |
| 9 | +0.06% | 35 / 36 | 97% | 920 / 1072 | 86% |
| 12 | +0.00% | 12 / 13 | 92% | 885 / 1032 | 86% |
| 19 | +0.03% | 3 / 3 | 100% | 882 / 1017 | 87% |

### html-500k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | -0.04% | 440 / 432 | 102% | 1175 / 1344 | 87% |
| 1 | -0.02% | 434 / 406 | 107% | 1125 / 1356 | 83% |
| 3 | +0.08% | 359 / 336 | 107% | 1136 / 1298 | 88% |
| 5 | +0.16% | 133 / 124 | 107% | 1093 / 1273 | 86% |
| 9 | +0.01% | 51 / 59 | 86% | 1219 / 1432 | 85% |
| 12 | +0.02% | 31 / 36 | 86% | 1267 / 1482 | 85% |
| 19 | +0.05% | 3 / 3 | 100% | 1170 / 1356 | 86% |

### text-2m

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | -0.04% | 294 / 240 | 123% | 772 / 894 | 86% |
| 1 | -0.03% | 288 / 240 | 120% | 816 / 982 | 83% |
| 3 | +0.07% | 254 / 214 | 119% | 846 / 971 | 87% |
| 5 | -0.11% | 89 / 87 | 102% | 747 / 858 | 87% |
| 9 | -0.03% | 32 / 38 | 84% | 892 / 1046 | 85% |
| 12 | -0.04% | 14 / 18 | 78% | 975 / 1126 | 87% |
| 19 | +0.03% | 2 / 2 | 100% | 957 / 1090 | 88% |

### words

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | -0.01% | 286 / 299 | 96% | 764 / 913 | 84% |
| 1 | +0.04% | 283 / 276 | 103% | 679 / 914 | 74% |
| 3 | +0.10% | 159 / 146 | 109% | 558 / 694 | 80% |
| 5 | +0.46% | 55 / 78 | 71% | 516 / 615 | 84% |
| 9 | +0.23% | 19 / 38 | 50% | 566 / 685 | 83% |
| 12 | +0.20% | 13 / 22 | 59% | 559 / 665 | 84% |
| 19 | +0.09% | 3 / 3 | 100% | 489 / 570 | 86% |

Built for `x86-64-v3` (Haswell and newer: AVX2, BMI2) instead, compression at the fast levels
is 5 to 8% faster (`html-16k` level 1: 458 to 496 MB/s, level 3: 373 to 392; `text-2m` level
1: 291 to 311) and decompression -2 to +4%. To get this, build with
`RUSTFLAGS="-C target-cpu=x86-64-v3"`, or `-C target-cpu=native` for the machine you build on.

What differs from Apple silicon:

- Compression at the fast levels, the ones a web server uses, is at or ahead of this build of
  the reference: 101 to 120% at levels 1 and 3, and level with it on the 4 KB input.
- Decompression trails more, at 74 to 88% (on Apple silicon 90 to 106% for text and HTML): on
  x86-64 the reference decodes Huffman literals with hand-written assembly.
- The word list at levels 5 to 12 trails most (as low as 51%): the lazy searches wait on
  memory, which the reference hides with prefetch instructions that safe Rust doesn't have.
- Matches less than 16 bytes back are copied differently. On x86-64 the copy goes by whole
  periods of the pattern, out of line, which decodes the word list 11 to 31% faster than
  laying the pattern out a byte at a time. On Apple silicon the same code makes the compiler
  build the whole sequence loop worse, and decoding everything else 1.5 to 20% slower, so
  there the pattern is laid out a byte at a time (`copy_match` in `src/decode.rs`).

## What the numbers show

- Every strategy is the reference's own, and makes much the same choices: on these inputs the
  output is within half a percent of its size at every level. Of the benchmark's other inputs,
  the 4 KB and 200 KB ones differ most, up to 1.4% larger.
- Compression keeps close to the reference on HTML and generated text (102 to 108% at levels
  1 and 3). The word list, with a sequence every 12 bytes or so, is where both compression
  (level 1: 83%) and decompression (71 to 84%) trail most: the per-sequence work is where the
  reference is still faster.
- Decompression of text and HTML runs at 90 to 106% of the reference's speed.
- Incompressible input is spotted early, as the reference does: up to level 12 oxzstd runs at
  66 to 107% of its speed, and from level 16 faster than it.
- At levels 19 to 22 the optimal parser's tables take about as much memory as the
  reference's: 0.5 MiB more on HTML, about 1 MiB less on text and the word list.
