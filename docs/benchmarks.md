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
| -5 | -0.46% | 485 / 592 | 82% | 1120 / 1429 | 78% | 8.0 / 9.3 | 5.8 / 5.8 |
| -1 | +0.04% | 389 / 472 | 82% | 927 / 1150 | 81% | 7.7 / 9.0 | 5.5 / 5.5 |
| 1 | +0.12% | 377 / 489 | 77% | 841 / 1096 | 77% | 6.5 / 8.6 | 5.4 / 5.4 |
| 3 | +0.10% | 228 / 237 | 96% | 706 / 881 | 80% | 7.2 / 9.0 | 5.4 / 6.8 |
| 5 | +0.46% | 128 / 131 | 98% | 616 / 802 | 77% | 9.5 / 10.6 | 5.3 / 6.6 |
| 7 | +0.22% | 65 / 78 | 83% | 650 / 849 | 77% | 11.4 / 12.9 | 5.3 / 6.6 |
| 9 | +0.23% | 47 / 59 | 80% | 660 / 879 | 75% | 16.3 / 17.8 | 5.2 / 6.7 |
| 12 | +0.20% | 25 / 32 | 78% | 665 / 888 | 75% | 46.3 / 47.9 | 5.3 / 6.7 |
| 16 | -0.34% | 11 / 12 | 92% | 537 / 786 | 68% | 38.4 / 39.8 | 5.2 / 6.7 |
| 19 | +0.09% | 5 / 7 | 71% | 508 / 748 | 68% | 55.4 / 56.3 | 5.2 / 6.7 |
| 22 | +0.04% | 4 / 6 | 67% | 509 / 751 | 68% | 71.4 / 72.3 | 5.1 / 6.7 |

### html-500k


| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd | Compress RAM MiB | Decompress RAM MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | -0.24% | 618 / 612 | 101% | 1680 / 1798 | 93% | 3.5 / 3.5 | 3.0 / 3.3 |
| -1 | -0.01% | 578 / 576 | 100% | 1539 / 1671 | 92% | 3.5 / 3.5 | 2.5 / 3.3 |
| 1 | +0.00% | 554 / 545 | 102% | 1455 / 1691 | 86% | 3.5 / 3.5 | 2.5 / 3.3 |
| 3 | +0.08% | 462 / 469 | 99% | 1498 / 1633 | 92% | 4.2 / 4.2 | 2.5 / 3.3 |
| 5 | +0.16% | 209 / 202 | 103% | 1439 / 1565 | 92% | 6.4 / 5.9 | 2.5 / 3.3 |
| 7 | -0.03% | 104 / 122 | 85% | 1581 / 1688 | 94% | 8.4 / 8.4 | 2.5 / 3.3 |
| 9 | +0.01% | 75 / 90 | 83% | 1600 / 1772 | 90% | 8.4 / 8.4 | 2.5 / 3.3 |
| 12 | +0.02% | 41 / 48 | 85% | 1659 / 1864 | 89% | 8.4 / 8.4 | 2.5 / 3.3 |
| 16 | +0.06% | 9 / 8 | 112% | 1605 / 1688 | 95% | 11.3 / 11.5 | 2.5 / 3.3 |
| 19 | +0.05% | 4 / 5 | 80% | 1522 / 1670 | 91% | 12.6 / 12.1 | 2.5 / 3.3 |
| 22 | +0.05% | 4 / 5 | 80% | 1527 / 1667 | 92% | 12.6 / 12.1 | 2.5 / 3.3 |

### text-2m


| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd | Compress RAM MiB | Decompress RAM MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | -0.12% | 391 / 382 | 102% | 1147 / 1141 | 101% | 5.7 / 7.5 | 6.6 / 5.0 |
| -1 | -0.02% | 371 / 360 | 103% | 1078 / 1089 | 99% | 5.9 / 7.4 | 6.5 / 5.0 |
| 1 | -0.02% | 363 / 353 | 103% | 1145 / 1203 | 95% | 5.9 / 7.1 | 6.4 / 4.9 |
| 3 | +0.07% | 320 / 322 | 99% | 1222 / 1231 | 99% | 6.3 / 7.7 | 6.4 / 6.1 |
| 5 | -0.11% | 133 / 131 | 102% | 1055 / 1060 | 100% | 8.5 / 9.6 | 6.3 / 6.1 |
| 7 | -0.01% | 60 / 73 | 82% | 1211 / 1228 | 99% | 10.5 / 12.0 | 6.3 / 6.0 |
| 9 | -0.03% | 43 / 54 | 80% | 1257 / 1295 | 97% | 15.5 / 17.0 | 6.4 / 6.0 |
| 12 | -0.04% | 19 / 23 | 83% | 1395 / 1466 | 95% | 25.4 / 26.9 | 6.3 / 6.1 |
| 16 | +0.06% | 5 / 5 | 100% | 1420 / 1484 | 96% | 36.8 / 38.9 | 6.3 / 5.9 |
| 19 | +0.03% | 3 / 4 | 75% | 1417 / 1470 | 96% | 38.6 / 39.4 | 6.3 / 5.9 |
| 22 | +0.03% | 3 / 4 | 75% | 1417 / 1459 | 97% | 38.5 / 39.4 | 6.3 / 5.9 |

## Speed and size, every input

### words (2,493,885 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 1,429,304 | 1,435,949 | -0.46% | 485 / 592 | 82% | 1120 / 1429 | 78% |
| -1 | 1,128,480 | 1,127,996 | +0.04% | 389 / 472 | 82% | 927 / 1150 | 81% |
| 1 | 890,804 | 889,777 | +0.12% | 377 / 489 | 77% | 841 / 1096 | 77% |
| 2 | 864,265 | 863,511 | +0.09% | 294 / 297 | 99% | 754 / 959 | 79% |
| 3 | 853,674 | 852,806 | +0.10% | 228 / 237 | 96% | 706 / 881 | 80% |
| 4 | 855,293 | 854,288 | +0.12% | 228 / 236 | 97% | 700 / 864 | 81% |
| 5 | 802,409 | 798,716 | +0.46% | 128 / 131 | 98% | 616 / 802 | 77% |
| 6 | 770,771 | 767,493 | +0.43% | 86 / 90 | 96% | 645 / 851 | 76% |
| 7 | 770,232 | 768,540 | +0.22% | 65 / 78 | 83% | 650 / 849 | 77% |
| 8 | 749,333 | 747,610 | +0.23% | 48 / 59 | 81% | 659 / 880 | 75% |
| 9 | 749,924 | 748,233 | +0.23% | 47 / 59 | 80% | 660 / 879 | 75% |
| 12 | 752,261 | 750,736 | +0.20% | 25 / 32 | 78% | 665 / 888 | 75% |
| 16 | 674,893 | 677,167 | -0.34% | 11 / 12 | 92% | 537 / 786 | 68% |
| 19 | 662,516 | 661,923 | +0.09% | 5 / 7 | 71% | 508 / 748 | 68% |
| 22 | 662,135 | 661,855 | +0.04% | 4 / 6 | 67% | 509 / 751 | 68% |

### text-2m (2,000,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 740,783 | 741,684 | -0.12% | 391 / 382 | 102% | 1147 / 1141 | 101% |
| -1 | 670,744 | 670,894 | -0.02% | 371 / 360 | 103% | 1078 / 1089 | 99% |
| 1 | 601,693 | 601,830 | -0.02% | 363 / 353 | 103% | 1145 / 1203 | 95% |
| 2 | 602,781 | 602,850 | -0.01% | 360 / 358 | 101% | 1018 / 1022 | 100% |
| 3 | 569,546 | 569,150 | +0.07% | 320 / 322 | 99% | 1222 / 1231 | 99% |
| 4 | 566,181 | 565,958 | +0.04% | 326 / 326 | 100% | 1236 / 1233 | 100% |
| 5 | 572,858 | 573,488 | -0.11% | 133 / 131 | 102% | 1055 / 1060 | 100% |
| 6 | 559,758 | 560,444 | -0.12% | 83 / 85 | 98% | 1144 / 1156 | 99% |
| 7 | 550,766 | 550,823 | -0.01% | 60 / 73 | 82% | 1211 / 1228 | 99% |
| 8 | 549,589 | 549,737 | -0.03% | 43 / 54 | 80% | 1257 / 1307 | 96% |
| 9 | 549,435 | 549,586 | -0.03% | 43 / 54 | 80% | 1257 / 1295 | 97% |
| 12 | 523,830 | 524,029 | -0.04% | 19 / 23 | 83% | 1395 / 1466 | 95% |
| 16 | 462,173 | 461,886 | +0.06% | 5 / 5 | 100% | 1420 / 1484 | 96% |
| 19 | 459,178 | 459,028 | +0.03% | 3 / 4 | 75% | 1417 / 1470 | 96% |
| 22 | 459,178 | 459,028 | +0.03% | 3 / 4 | 75% | 1417 / 1459 | 97% |

### html-500k (500,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 132,355 | 132,678 | -0.24% | 618 / 612 | 101% | 1680 / 1798 | 93% |
| -1 | 118,954 | 118,970 | -0.01% | 578 / 576 | 100% | 1539 / 1671 | 92% |
| 1 | 105,976 | 105,973 | +0.00% | 554 / 545 | 102% | 1455 / 1691 | 86% |
| 2 | 103,838 | 103,845 | -0.01% | 527 / 508 | 104% | 1350 / 1510 | 89% |
| 3 | 98,779 | 98,697 | +0.08% | 462 / 469 | 99% | 1498 / 1633 | 92% |
| 4 | 98,610 | 98,535 | +0.08% | 460 / 474 | 97% | 1499 / 1634 | 92% |
| 5 | 93,700 | 93,549 | +0.16% | 209 / 202 | 103% | 1439 / 1565 | 92% |
| 6 | 91,615 | 91,705 | -0.10% | 138 / 136 | 101% | 1547 / 1660 | 93% |
| 7 | 90,558 | 90,585 | -0.03% | 104 / 122 | 85% | 1581 / 1688 | 94% |
| 8 | 90,903 | 90,892 | +0.01% | 75 / 89 | 84% | 1606 / 1775 | 90% |
| 9 | 90,903 | 90,892 | +0.01% | 75 / 90 | 83% | 1600 / 1772 | 90% |
| 12 | 89,261 | 89,240 | +0.02% | 41 / 48 | 85% | 1659 / 1864 | 89% |
| 16 | 82,840 | 82,794 | +0.06% | 9 / 8 | 112% | 1605 / 1688 | 95% |
| 19 | 82,352 | 82,310 | +0.05% | 4 / 5 | 80% | 1522 / 1670 | 91% |
| 22 | 82,352 | 82,310 | +0.05% | 4 / 5 | 80% | 1527 / 1667 | 92% |

### skewed-300k (300,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 100,518 | 100,313 | +0.20% | 410 / 413 | 99% | 839 / 927 | 91% |
| -1 | 90,757 | 90,545 | +0.23% | 384 / 390 | 98% | 778 / 869 | 90% |
| 1 | 80,717 | 80,700 | +0.02% | 388 / 392 | 99% | 907 / 992 | 91% |
| 2 | 82,935 | 82,824 | +0.13% | 360 / 367 | 98% | 756 / 847 | 89% |
| 3 | 80,194 | 80,152 | +0.05% | 340 / 350 | 97% | 981 / 1048 | 94% |
| 4 | 80,088 | 80,114 | -0.03% | 337 / 344 | 98% | 980 / 1044 | 94% |
| 5 | 80,422 | 80,404 | +0.02% | 138 / 126 | 110% | 834 / 928 | 90% |
| 6 | 76,713 | 76,715 | -0.00% | 80 / 75 | 107% | 951 / 1053 | 90% |
| 7 | 75,840 | 75,847 | -0.01% | 56 / 65 | 86% | 1053 / 1121 | 94% |
| 8 | 75,029 | 75,026 | +0.00% | 41 / 48 | 85% | 1086 / 1183 | 92% |
| 9 | 75,029 | 75,026 | +0.00% | 41 / 47 | 87% | 1086 / 1191 | 91% |
| 12 | 73,414 | 73,409 | +0.01% | 17 / 20 | 85% | 1247 / 1337 | 93% |
| 16 | 61,686 | 61,685 | +0.00% | 5 / 6 | 83% | 1427 / 2055 | 69% |
| 19 | 64,315 | 64,315 | +0.00% | 3 / 3 | 100% | 1371 / 1701 | 81% |
| 22 | 64,315 | 64,315 | +0.00% | 3 / 3 | 100% | 1373 / 1705 | 81% |

### random-100k (100,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 100,012 | 100,012 | +0.00% | 14286 / 15072 | 95% | 68587 / 33529 | 205% |
| -1 | 100,012 | 100,012 | +0.00% | 13484 / 14283 | 94% | 68587 / 29407 | 233% |
| 1 | 100,012 | 100,012 | +0.00% | 9126 / 10136 | 90% | 68587 / 33064 | 207% |
| 2 | 100,012 | 100,012 | +0.00% | 8362 / 9553 | 88% | 68587 / 32360 | 212% |
| 3 | 100,012 | 100,012 | +0.00% | 6015 / 7591 | 79% | 68540 / 32515 | 211% |
| 4 | 100,012 | 100,012 | +0.00% | 4848 / 7442 | 65% | 68587 / 29672 | 231% |
| 5 | 100,012 | 100,012 | +0.00% | 3922 / 4246 | 92% | 66667 / 30303 | 220% |
| 6 | 100,012 | 100,012 | +0.00% | 4007 / 4278 | 94% | 68587 / 35741 | 192% |
| 7 | 100,012 | 100,012 | +0.00% | 3947 / 4195 | 94% | 68587 / 52958 | 130% |
| 8 | 100,012 | 100,012 | +0.00% | 3648 / 4244 | 86% | 68587 / 54480 | 126% |
| 9 | 100,012 | 100,012 | +0.00% | 3380 / 3776 | 90% | 68587 / 61540 | 111% |
| 12 | 100,012 | 100,012 | +0.00% | 741 / 761 | 97% | 68587 / 56604 | 121% |
| 16 | 100,012 | 100,012 | +0.00% | 53 / 45 | 118% | 68587 / 53486 | 128% |
| 19 | 100,012 | 100,012 | +0.00% | 27 / 23 | 117% | 96061 / 35013 | 274% |
| 22 | 100,012 | 100,012 | +0.00% | 27 / 23 | 117% | 96061 / 42292 | 227% |

## x86_64

The tables above are on Apple silicon. These are on an x86_64 Linux virtual machine with AVX2
and BMI2, one core per run. The reference is `zstd` 1.5.4; oxzstd is built with Rust 1.99.1
for the default x86-64 target, with its own release profile. Same benchmark, same method;
`html-4k` and `html-16k` are the benchmark's small inputs, the size of most web responses.
These numbers predate the faster compression at levels 1 and 3 in the Apple silicon tables
above, and haven't been measured again since.

### html-4k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | +0.00% | 432 / 487 | 89% | 5780 / 1236 | 468% |
| 1 | -0.10% | 306 / 344 | 89% | 769 / 745 | 103% |
| 3 | +0.10% | 282 / 301 | 94% | 695 / 737 | 94% |
| 5 | +0.00% | 142 / 156 | 91% | 788 / 759 | 104% |
| 9 | +0.31% | 74 / 63 | 117% | 696 / 759 | 92% |
| 12 | +0.10% | 16 / 17 | 94% | 682 / 764 | 89% |
| 19 | +0.21% | 3 / 3 | 100% | 664 / 748 | 89% |

### html-16k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | +0.05% | 477 / 495 | 96% | 938 / 1257 | 75% |
| 1 | +0.11% | 396 / 413 | 96% | 784 / 1028 | 76% |
| 3 | +0.06% | 342 / 362 | 94% | 735 / 993 | 74% |
| 5 | +0.09% | 122 / 130 | 94% | 797 / 1034 | 77% |
| 9 | +0.06% | 35 / 34 | 103% | 804 / 1087 | 74% |
| 12 | +0.00% | 12 / 13 | 92% | 786 / 1062 | 74% |
| 19 | +0.03% | 3 / 3 | 100% | 775 / 1039 | 75% |

### html-500k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | -0.01% | 376 / 428 | 88% | 990 / 1324 | 75% |
| 1 | +0.00% | 379 / 404 | 94% | 957 / 1377 | 69% |
| 3 | -0.02% | 329 / 341 | 96% | 958 / 1327 | 72% |
| 5 | +0.17% | 130 / 114 | 114% | 909 / 1281 | 71% |
| 9 | +0.11% | 48 / 57 | 84% | 1048 / 1410 | 74% |
| 12 | +0.05% | 29 / 33 | 88% | 1095 / 1480 | 74% |
| 19 | -0.14% | 3 / 3 | 100% | 1004 / 1341 | 75% |

### text-2m

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | -0.02% | 232 / 274 | 85% | 621 / 897 | 69% |
| 1 | -0.02% | 233 / 274 | 85% | 672 / 984 | 68% |
| 3 | -0.01% | 248 / 236 | 105% | 711 / 937 | 76% |
| 5 | -0.10% | 87 / 89 | 98% | 610 / 860 | 71% |
| 9 | +0.16% | 28 / 38 | 74% | 735 / 1052 | 70% |
| 12 | +0.01% | 11 / 17 | 65% | 839 / 1122 | 75% |
| 19 | +0.03% | 2 / 2 | 100% | 830 / 1081 | 77% |

### words

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|
| -1 | +0.04% | 235 / 296 | 79% | 585 / 904 | 65% |
| 1 | +0.07% | 239 / 270 | 89% | 550 / 895 | 61% |
| 3 | +0.05% | 144 / 149 | 97% | 443 / 698 | 63% |
| 5 | +0.33% | 53 / 73 | 73% | 368 / 600 | 61% |
| 9 | +0.10% | 17 / 36 | 47% | 427 / 676 | 63% |
| 12 | +0.08% | 11 / 18 | 61% | 425 / 676 | 63% |
| 19 | +0.10% | 2 / 3 | 67% | 337 / 560 | 60% |

Built for `x86-64-v3` (Haswell and newer: AVX2, BMI2) instead, compression at the fast levels
is 3 to 7% faster (`html-16k` level 1: 396 to 416 MB/s, level 3: 342 to 360; `text-2m` level
1: 233 to 248) and decompression is unchanged. To get this, build with
`RUSTFLAGS="-C target-cpu=x86-64-v3"`, or `-C target-cpu=native` for the machine you build on.

What differs from Apple silicon:

- Compression at the fast levels, the ones a web server uses, is much the same: 85 to 105% of
  the reference at levels 1 and 3, and level with it on the small inputs.
- Decompression trails more, at 60 to 80% (level with the reference on the 4 KB input): on
  x86-64 the reference decodes Huffman literals with hand-written assembly.
- The word list at levels 5 to 12 trails most (as low as 47%): the lazy searches wait on
  memory, which the reference hides with prefetch instructions that safe Rust doesn't have.

## What the numbers show

- Every strategy is the reference's own, and makes much the same choices: the output is within
  half a percent of its size at every level but 13 to 15, where `btlazy2` searches the binary
  tree differently (2.4% smaller to 1.7% larger).
- Compression keeps close to the reference on HTML and generated text (97 to 103% at levels 1
  and 3). The word list, with a sequence every 12 bytes or so, is where both compression
  (level 1: 77%) and decompression (68 to 81%) trail most: the per-sequence work is where the
  reference is still faster.
- Incompressible input is spotted early, as the reference does: up to level 12 oxzstd runs at
  65 to 97% of its speed, and from level 16 faster than it.
- At levels 19 to 22 the optimal parser's tables take about as much memory as the
  reference's: 0.5 MiB more on HTML, about 1 MiB less on text and the word list.
