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
| -5 | -0.46% | 490 / 577 | 85% | 1154 / 1392 | 83% | 7.9 / 9.3 | 5.8 / 5.7 |
| -1 | +0.04% | 394 / 464 | 85% | 955 / 1125 | 85% | 7.7 / 9.0 | 5.5 / 5.5 |
| 1 | +0.12% | 376 / 474 | 79% | 859 / 1059 | 81% | 6.4 / 8.5 | 5.4 / 5.3 |
| 3 | +0.10% | 234 / 231 | 101% | 723 / 864 | 84% | 7.2 / 8.9 | 5.3 / 6.8 |
| 5 | +0.46% | 139 / 126 | 110% | 637 / 779 | 82% | 9.5 / 10.6 | 5.2 / 6.6 |
| 7 | +0.22% | 69 / 76 | 91% | 671 / 821 | 82% | 11.3 / 12.9 | 5.3 / 6.6 |
| 9 | +0.23% | 50 / 57 | 88% | 682 / 864 | 79% | 16.3 / 17.9 | 5.2 / 6.7 |
| 12 | +0.20% | 29 / 31 | 94% | 690 / 874 | 79% | 46.3 / 47.9 | 5.2 / 6.7 |
| 16 | -0.34% | 12 / 12 | 100% | 561 / 774 | 72% | 38.5 / 39.8 | 5.2 / 6.7 |
| 19 | +0.09% | 5 / 6 | 83% | 531 / 746 | 71% | 55.3 / 56.3 | 5.2 / 6.7 |
| 22 | +0.04% | 4 / 6 | 67% | 531 / 735 | 72% | 71.3 / 72.3 | 5.2 / 6.7 |

### html-500k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd | Compress RAM MiB | Decompress RAM MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | -0.24% | 651 / 640 | 102% | 1754 / 1862 | 94% | 3.5 / 3.5 | 3.0 / 3.3 |
| -1 | -0.01% | 591 / 581 | 102% | 1599 / 1698 | 94% | 3.5 / 3.5 | 2.5 / 3.3 |
| 1 | +0.00% | 564 / 546 | 103% | 1503 / 1722 | 87% | 3.5 / 3.5 | 2.5 / 3.3 |
| 3 | +0.08% | 468 / 470 | 100% | 1553 / 1656 | 94% | 4.2 / 4.2 | 2.5 / 3.3 |
| 5 | +0.16% | 227 / 208 | 109% | 1486 / 1591 | 93% | 6.4 / 5.9 | 2.5 / 3.3 |
| 7 | -0.03% | 108 / 121 | 89% | 1636 / 1718 | 95% | 8.4 / 8.4 | 2.5 / 3.3 |
| 9 | +0.01% | 79 / 92 | 86% | 1660 / 1793 | 93% | 8.4 / 8.4 | 2.5 / 3.3 |
| 12 | +0.02% | 49 / 48 | 102% | 1718 / 1879 | 91% | 8.4 / 8.4 | 2.5 / 3.3 |
| 16 | +0.06% | 9 / 9 | 100% | 1670 / 1712 | 98% | 11.3 / 11.5 | 2.5 / 3.3 |
| 19 | +0.05% | 4 / 5 | 80% | 1585 / 1690 | 94% | 12.6 / 12.1 | 2.5 / 3.3 |
| 22 | +0.05% | 4 / 5 | 80% | 1586 / 1690 | 94% | 12.6 / 12.1 | 2.5 / 3.3 |

### text-2m

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd | Compress RAM MiB | Decompress RAM MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | -0.12% | 409 / 385 | 106% | 1179 / 1161 | 102% | 5.7 / 7.5 | 6.6 / 5.0 |
| -1 | -0.02% | 387 / 363 | 107% | 1109 / 1094 | 101% | 5.9 / 7.4 | 6.5 / 5.0 |
| 1 | -0.02% | 373 / 356 | 105% | 1169 / 1197 | 98% | 5.9 / 7.1 | 6.4 / 4.9 |
| 3 | +0.07% | 327 / 321 | 102% | 1255 / 1242 | 101% | 6.3 / 7.7 | 6.4 / 6.1 |
| 5 | -0.11% | 144 / 134 | 107% | 1086 / 1065 | 102% | 8.5 / 9.6 | 6.4 / 6.1 |
| 7 | -0.01% | 61 / 74 | 82% | 1243 / 1232 | 101% | 10.5 / 12.0 | 6.4 / 6.0 |
| 9 | -0.03% | 44 / 55 | 80% | 1287 / 1306 | 99% | 15.5 / 17.0 | 6.4 / 6.0 |
| 12 | -0.04% | 22 / 24 | 92% | 1424 / 1487 | 96% | 25.5 / 26.9 | 6.3 / 6.1 |
| 16 | +0.06% | 5 / 5 | 100% | 1452 / 1506 | 96% | 36.8 / 38.9 | 6.3 / 5.9 |
| 19 | +0.03% | 3 / 4 | 75% | 1448 / 1490 | 97% | 38.6 / 39.4 | 6.3 / 5.9 |
| 22 | +0.03% | 3 / 4 | 75% | 1448 / 1473 | 98% | 38.6 / 39.4 | 6.3 / 5.9 |

## Speed and size, every input

### words (2,493,885 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 1,429,304 | 1,435,949 | -0.46% | 490 / 577 | 85% | 1154 / 1392 | 83% |
| -1 | 1,128,480 | 1,127,996 | +0.04% | 394 / 464 | 85% | 955 / 1125 | 85% |
| 1 | 890,804 | 889,777 | +0.12% | 376 / 474 | 79% | 859 / 1059 | 81% |
| 2 | 864,265 | 863,511 | +0.09% | 299 / 290 | 103% | 774 / 943 | 82% |
| 3 | 853,674 | 852,806 | +0.10% | 234 / 231 | 101% | 723 / 864 | 84% |
| 4 | 855,293 | 854,288 | +0.12% | 235 / 230 | 102% | 719 / 853 | 84% |
| 5 | 802,409 | 798,716 | +0.46% | 139 / 126 | 110% | 637 / 779 | 82% |
| 6 | 770,771 | 767,493 | +0.43% | 91 / 87 | 105% | 663 / 825 | 80% |
| 7 | 770,232 | 768,540 | +0.22% | 69 / 76 | 91% | 671 / 821 | 82% |
| 8 | 749,333 | 747,610 | +0.23% | 53 / 57 | 93% | 678 / 867 | 78% |
| 9 | 749,924 | 748,233 | +0.23% | 50 / 57 | 88% | 682 / 864 | 79% |
| 12 | 752,261 | 750,736 | +0.20% | 29 / 31 | 94% | 690 / 874 | 79% |
| 16 | 674,893 | 677,167 | -0.34% | 12 / 12 | 100% | 561 / 774 | 72% |
| 19 | 662,516 | 661,923 | +0.09% | 5 / 6 | 83% | 531 / 746 | 71% |
| 22 | 662,135 | 661,855 | +0.04% | 4 / 6 | 67% | 531 / 735 | 72% |

### text-2m (2,000,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 740,783 | 741,684 | -0.12% | 409 / 385 | 106% | 1179 / 1161 | 102% |
| -1 | 670,744 | 670,894 | -0.02% | 387 / 363 | 107% | 1109 / 1094 | 101% |
| 1 | 601,693 | 601,830 | -0.02% | 373 / 356 | 105% | 1169 / 1197 | 98% |
| 2 | 602,781 | 602,850 | -0.01% | 376 / 360 | 104% | 1047 / 1036 | 101% |
| 3 | 569,546 | 569,150 | +0.07% | 327 / 321 | 102% | 1255 / 1242 | 101% |
| 4 | 566,181 | 565,958 | +0.04% | 332 / 329 | 101% | 1270 / 1234 | 103% |
| 5 | 572,858 | 573,488 | -0.11% | 144 / 134 | 107% | 1086 / 1065 | 102% |
| 6 | 559,758 | 560,444 | -0.12% | 91 / 86 | 106% | 1174 / 1169 | 100% |
| 7 | 550,766 | 550,823 | -0.01% | 61 / 74 | 82% | 1243 / 1232 | 101% |
| 8 | 549,589 | 549,737 | -0.03% | 44 / 54 | 81% | 1286 / 1315 | 98% |
| 9 | 549,435 | 549,586 | -0.03% | 44 / 55 | 80% | 1287 / 1306 | 99% |
| 12 | 523,830 | 524,029 | -0.04% | 22 / 24 | 92% | 1424 / 1487 | 96% |
| 16 | 462,173 | 461,886 | +0.06% | 5 / 5 | 100% | 1452 / 1506 | 96% |
| 19 | 459,178 | 459,028 | +0.03% | 3 / 4 | 75% | 1448 / 1490 | 97% |
| 22 | 459,178 | 459,028 | +0.03% | 3 / 4 | 75% | 1448 / 1473 | 98% |

### html-500k (500,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 132,355 | 132,678 | -0.24% | 651 / 640 | 102% | 1754 / 1862 | 94% |
| -1 | 118,954 | 118,970 | -0.01% | 591 / 581 | 102% | 1599 / 1698 | 94% |
| 1 | 105,976 | 105,973 | +0.00% | 564 / 546 | 103% | 1503 / 1722 | 87% |
| 2 | 103,838 | 103,845 | -0.01% | 534 / 526 | 102% | 1389 / 1534 | 91% |
| 3 | 98,779 | 98,697 | +0.08% | 468 / 470 | 100% | 1553 / 1656 | 94% |
| 4 | 98,610 | 98,535 | +0.08% | 467 / 472 | 99% | 1545 / 1659 | 93% |
| 5 | 93,700 | 93,549 | +0.16% | 227 / 208 | 109% | 1486 / 1591 | 93% |
| 6 | 91,615 | 91,705 | -0.10% | 149 / 143 | 104% | 1592 / 1682 | 95% |
| 7 | 90,558 | 90,585 | -0.03% | 108 / 121 | 89% | 1636 / 1718 | 95% |
| 8 | 90,903 | 90,892 | +0.01% | 79 / 92 | 86% | 1658 / 1795 | 92% |
| 9 | 90,903 | 90,892 | +0.01% | 79 / 92 | 86% | 1660 / 1793 | 93% |
| 12 | 89,261 | 89,240 | +0.02% | 49 / 48 | 102% | 1718 / 1879 | 91% |
| 16 | 82,840 | 82,794 | +0.06% | 9 / 9 | 100% | 1670 / 1712 | 98% |
| 19 | 82,352 | 82,310 | +0.05% | 4 / 5 | 80% | 1585 / 1690 | 94% |
| 22 | 82,352 | 82,310 | +0.05% | 4 / 5 | 80% | 1586 / 1690 | 94% |

### skewed-300k (300,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 100,518 | 100,313 | +0.20% | 437 / 418 | 105% | 869 / 928 | 94% |
| -1 | 90,757 | 90,545 | +0.23% | 409 / 377 | 108% | 802 / 849 | 94% |
| 1 | 80,717 | 80,700 | +0.02% | 408 / 388 | 105% | 933 / 994 | 94% |
| 2 | 82,935 | 82,824 | +0.13% | 380 / 367 | 104% | 783 / 853 | 92% |
| 3 | 80,194 | 80,152 | +0.05% | 353 / 349 | 101% | 1013 / 1054 | 96% |
| 4 | 80,088 | 80,114 | -0.03% | 349 / 353 | 99% | 1011 / 1044 | 97% |
| 5 | 80,422 | 80,404 | +0.02% | 151 / 127 | 119% | 864 / 930 | 93% |
| 6 | 76,713 | 76,715 | -0.00% | 89 / 75 | 119% | 981 / 1052 | 93% |
| 7 | 75,840 | 75,847 | -0.01% | 57 / 65 | 88% | 1082 / 1139 | 95% |
| 8 | 75,029 | 75,026 | +0.00% | 42 / 48 | 88% | 1116 / 1202 | 93% |
| 9 | 75,029 | 75,026 | +0.00% | 42 / 48 | 88% | 1116 / 1194 | 93% |
| 12 | 73,414 | 73,409 | +0.01% | 21 / 20 | 105% | 1281 / 1353 | 95% |
| 16 | 61,686 | 61,685 | +0.00% | 5 / 6 | 83% | 1402 / 2057 | 68% |
| 19 | 64,315 | 64,315 | +0.00% | 3 / 3 | 100% | 1399 / 1712 | 82% |
| 22 | 64,315 | 64,315 | +0.00% | 3 / 3 | 100% | 1402 / 1706 | 82% |

### random-100k (100,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 100,012 | 100,012 | +0.00% | 15094 / 15157 | 100% | 68587 / 31602 | 217% |
| -1 | 100,012 | 100,012 | +0.00% | 13333 / 14497 | 92% | 68540 / 34212 | 200% |
| 1 | 100,012 | 100,012 | +0.00% | 9160 / 10290 | 89% | 68587 / 38823 | 177% |
| 2 | 100,012 | 100,012 | +0.00% | 8333 / 9501 | 88% | 68587 / 40931 | 168% |
| 3 | 100,012 | 100,012 | +0.00% | 5955 / 7564 | 79% | 70621 / 39354 | 179% |
| 4 | 100,012 | 100,012 | +0.00% | 4898 / 7450 | 66% | 68587 / 32726 | 210% |
| 5 | 100,012 | 100,012 | +0.00% | 4152 / 4302 | 97% | 68587 / 33530 | 205% |
| 6 | 100,012 | 100,012 | +0.00% | 4013 / 4328 | 93% | 68587 / 33306 | 206% |
| 7 | 100,012 | 100,012 | +0.00% | 4138 / 4269 | 97% | 68587 / 46853 | 146% |
| 8 | 100,012 | 100,012 | +0.00% | 3762 / 4258 | 88% | 68587 / 43916 | 156% |
| 9 | 100,012 | 100,012 | +0.00% | 3468 / 3771 | 92% | 68587 / 45285 | 151% |
| 12 | 100,012 | 100,012 | +0.00% | 741 / 770 | 96% | 68587 / 33043 | 208% |
| 16 | 100,012 | 100,012 | +0.00% | 53 / 45 | 118% | 68587 / 31046 | 221% |
| 19 | 100,012 | 100,012 | +0.00% | 27 / 23 | 117% | 96061 / 35564 | 270% |
| 22 | 100,012 | 100,012 | +0.00% | 27 / 23 | 117% | 96061 / 33735 | 285% |

## x86_64

The tables above are on Apple silicon. These are on an x86_64 Linux virtual machine (QEMU,
AVX2 and BMI2), one core per run, in a Debian container. The reference is Debian's `zstd`
1.5.7; oxzstd is built with Rust 1.99.1 for the default x86-64 target, with its own release
profile. Same benchmark, same method; `html-4k` and `html-16k` are the benchmark's small
inputs, the size of most web responses.

### html-4k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | +0.00% | 470 / 479 | 98% | 1048 / 1174 | 89% |
| 1 | -0.10% | 347 / 345 | 101% | 696 / 819 | 85% |
| 3 | +0.10% | 296 / 294 | 101% | 656 / 722 | 91% |
| 5 | +0.00% | 157 / 158 | 99% | 684 / 806 | 85% |
| 9 | +0.31% | 76 / 66 | 115% | 684 / 807 | 85% |
| 12 | +0.00% | 17 / 17 | 100% | 674 / 802 | 84% |
| 19 | +0.21% | 3 / 3 | 100% | 671 / 780 | 86% |

### html-16k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | +0.05% | 528 / 486 | 109% | 1062 / 1201 | 88% |
| 1 | +0.11% | 446 / 398 | 112% | 877 / 1014 | 86% |
| 3 | +0.17% | 367 / 336 | 109% | 845 / 968 | 87% |
| 5 | +0.09% | 129 / 130 | 99% | 890 / 997 | 89% |
| 9 | +0.06% | 35 / 36 | 97% | 915 / 1066 | 86% |
| 12 | +0.00% | 12 / 13 | 92% | 880 / 1027 | 86% |
| 19 | +0.03% | 3 / 3 | 100% | 884 / 1011 | 87% |

### html-500k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | -0.01% | 440 / 425 | 104% | 1174 / 1322 | 89% |
| 1 | +0.00% | 426 / 403 | 106% | 1117 / 1350 | 83% |
| 3 | +0.08% | 356 / 336 | 106% | 1125 / 1294 | 87% |
| 5 | +0.16% | 118 / 122 | 97% | 1085 / 1251 | 87% |
| 9 | +0.01% | 49 / 57 | 86% | 1207 / 1413 | 85% |
| 12 | +0.02% | 33 / 34 | 97% | 1261 / 1443 | 87% |
| 19 | +0.05% | 3 / 3 | 100% | 1152 / 1354 | 85% |

### text-2m

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | -0.02% | 286 / 236 | 121% | 758 / 864 | 88% |
| 1 | -0.02% | 280 / 236 | 119% | 814 / 951 | 86% |
| 3 | +0.07% | 251 / 200 | 125% | 822 / 967 | 85% |
| 5 | -0.11% | 87 / 85 | 102% | 745 / 846 | 88% |
| 9 | -0.03% | 31 / 37 | 84% | 885 / 1019 | 87% |
| 12 | -0.04% | 14 / 17 | 82% | 975 / 1096 | 89% |
| 19 | +0.03% | 2 / 2 | 100% | 980 / 1076 | 91% |

### words

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | +0.04% | 280 / 304 | 92% | 746 / 913 | 82% |
| 1 | +0.12% | 276 / 285 | 97% | 683 / 902 | 76% |
| 3 | +0.10% | 168 / 150 | 112% | 570 / 692 | 82% |
| 5 | +0.46% | 59 / 77 | 77% | 524 / 616 | 85% |
| 9 | +0.23% | 21 / 38 | 55% | 569 / 683 | 83% |
| 12 | +0.20% | 13 / 22 | 59% | 568 / 685 | 83% |
| 19 | +0.09% | 3 / 3 | 100% | 479 / 520 | 92% |

Built for `x86-64-v3` (Haswell and newer: AVX2, BMI2) instead, compression at the fast levels
is 3 to 8% faster (`html-16k` level 1: 446 to 476 MB/s, level 3: 367 to 382; `text-2m` level
1: 280 to 303) and decompression 1 to 3%. To get this, build with
`RUSTFLAGS="-C target-cpu=x86-64-v3"`, or `-C target-cpu=native` for the machine you build on.

What differs from Apple silicon:

- Compression at the fast levels, the ones a web server uses, is at or ahead of this build of
  the reference: 97 to 126% at levels 1 and 3, and level with it on the 4 KB input.
- Decompression trails more, at 76 to 92% (on Apple silicon 87 to 103% for text and HTML): on
  x86-64 the reference decodes Huffman literals with hand-written assembly.
- The word list at levels 5 to 12 trails most (as low as 55%): the lazy searches wait on
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
- Compression keeps close to the reference on HTML and generated text (100 to 105% at levels
  1 and 3). The word list, with a sequence every 12 bytes or so, is where both compression
  (level 1: 79%) and decompression (71 to 85%) trail most: the per-sequence work is where the
  reference is still faster.
- Decompression of text and HTML runs at 87 to 103% of the reference's speed.
- Incompressible input is spotted early, as the reference does: up to level 12 oxzstd runs at
  66 to 100% of its speed, and from level 16 faster than it.
- At levels 19 to 22 the optimal parser's tables take about as much memory as the
  reference's: 0.5 MiB more on HTML, about 1 MiB less on text and the word list.
