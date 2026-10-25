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
| -5 | -0.46% | 418 / 589 | 71% | 1109 / 1409 | 79% | 8.0 / 9.3 | 6.0 / 5.8 |
| -1 | +0.04% | 332 / 470 | 71% | 917 / 1149 | 80% | 6.2 / 9.0 | 5.6 / 5.5 |
| 1 | +0.12% | 321 / 484 | 66% | 832 / 1076 | 77% | 6.3 / 8.5 | 5.5 / 5.3 |
| 3 | +0.10% | 212 / 236 | 90% | 696 / 876 | 79% | 6.9 / 8.9 | 5.4 / 6.7 |
| 5 | +0.46% | 127 / 130 | 98% | 609 / 785 | 78% | 9.1 / 10.6 | 5.4 / 6.6 |
| 7 | +0.22% | 64 / 77 | 83% | 644 / 843 | 76% | 11.1 / 12.9 | 5.3 / 6.6 |
| 9 | +0.23% | 47 / 59 | 80% | 656 / 888 | 74% | 16.0 / 17.8 | 5.3 / 6.7 |
| 12 | +0.20% | 25 / 33 | 76% | 661 / 887 | 75% | 45.9 / 47.8 | 5.3 / 6.7 |
| 16 | -0.34% | 12 / 12 | 100% | 536 / 782 | 69% | 38.1 / 39.8 | 5.3 / 6.7 |
| 19 | +0.09% | 5 / 7 | 71% | 505 / 751 | 67% | 59.0 / 56.3 | 5.3 / 6.6 |
| 22 | +0.04% | 4 / 6 | 67% | 506 / 745 | 68% | 88.6 / 72.3 | 5.2 / 6.6 |

### html-500k


| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd | Compress RAM MiB | Decompress RAM MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | -0.24% | 578 / 637 | 91% | 1645 / 1852 | 89% | 3.2 / 3.5 | 3.0 / 3.3 |
| -1 | -0.01% | 522 / 574 | 91% | 1505 / 1682 | 89% | 2.9 / 3.5 | 2.5 / 3.3 |
| 1 | +0.00% | 512 / 554 | 92% | 1415 / 1715 | 83% | 3.0 / 3.5 | 2.5 / 3.3 |
| 3 | +0.08% | 430 / 471 | 91% | 1469 / 1616 | 91% | 3.6 / 4.1 | 2.5 / 3.3 |
| 5 | +0.16% | 205 / 208 | 99% | 1406 / 1594 | 88% | 5.9 / 5.9 | 2.5 / 3.3 |
| 7 | -0.03% | 103 / 122 | 84% | 1552 / 1706 | 91% | 7.9 / 8.4 | 2.5 / 3.3 |
| 9 | +0.01% | 75 / 91 | 82% | 1564 / 1778 | 88% | 7.9 / 8.4 | 2.5 / 3.3 |
| 12 | +0.02% | 41 / 49 | 84% | 1604 / 1871 | 86% | 7.8 / 8.4 | 2.5 / 3.3 |
| 16 | +0.06% | 9 / 9 | 100% | 1569 / 1702 | 92% | 10.8 / 11.5 | 2.5 / 3.2 |
| 19 | +0.05% | 4 / 5 | 80% | 1490 / 1679 | 89% | 16.9 / 12.1 | 2.5 / 3.2 |
| 22 | +0.05% | 4 / 5 | 80% | 1494 / 1677 | 89% | 16.9 / 12.0 | 2.5 / 3.2 |

### text-2m


| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd | Compress RAM MiB | Decompress RAM MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | -0.12% | 356 / 384 | 93% | 1135 / 1158 | 98% | 5.3 / 7.5 | 6.7 / 5.0 |
| -1 | -0.02% | 336 / 363 | 93% | 1050 / 1085 | 97% | 5.2 / 7.3 | 6.5 / 5.0 |
| 1 | -0.02% | 328 / 353 | 93% | 1133 / 1198 | 95% | 5.2 / 7.1 | 6.5 / 4.9 |
| 3 | +0.07% | 314 / 321 | 98% | 1197 / 1209 | 99% | 5.9 / 7.7 | 6.5 / 6.0 |
| 5 | -0.11% | 131 / 134 | 98% | 1050 / 1066 | 98% | 8.0 / 9.5 | 6.4 / 6.0 |
| 7 | -0.01% | 60 / 74 | 81% | 1202 / 1243 | 97% | 10.1 / 12.0 | 6.4 / 6.0 |
| 9 | -0.03% | 43 / 55 | 78% | 1250 / 1305 | 96% | 15.1 / 17.0 | 6.4 / 6.0 |
| 12 | -0.04% | 19 / 24 | 79% | 1377 / 1450 | 95% | 25.0 / 26.9 | 6.4 / 6.1 |
| 16 | +0.06% | 5 / 5 | 100% | 1390 / 1492 | 93% | 36.4 / 38.9 | 6.4 / 6.5 |
| 19 | +0.03% | 3 / 4 | 75% | 1393 / 1478 | 94% | 39.4 / 39.4 | 6.4 / 5.9 |
| 22 | +0.03% | 4 / 4 | 100% | 1393 / 1489 | 94% | 39.4 / 39.4 | 6.4 / 5.9 |

## Speed and size, every input

### words (2,493,885 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 1,429,304 | 1,435,949 | -0.46% | 418 / 589 | 71% | 1109 / 1409 | 79% |
| -1 | 1,128,476 | 1,127,996 | +0.04% | 332 / 470 | 71% | 917 / 1149 | 80% |
| 1 | 890,801 | 889,777 | +0.12% | 321 / 484 | 66% | 832 / 1076 | 77% |
| 2 | 864,265 | 863,511 | +0.09% | 263 / 296 | 89% | 745 / 952 | 78% |
| 3 | 853,682 | 852,806 | +0.10% | 212 / 236 | 90% | 696 / 876 | 79% |
| 4 | 855,270 | 854,288 | +0.11% | 209 / 235 | 89% | 691 / 866 | 80% |
| 5 | 802,407 | 798,716 | +0.46% | 127 / 130 | 98% | 609 / 785 | 78% |
| 6 | 770,771 | 767,493 | +0.43% | 85 / 89 | 96% | 638 / 846 | 75% |
| 7 | 770,232 | 768,540 | +0.22% | 64 / 77 | 83% | 644 / 843 | 76% |
| 8 | 749,333 | 747,610 | +0.23% | 48 / 59 | 81% | 654 / 885 | 74% |
| 9 | 749,924 | 748,233 | +0.23% | 47 / 59 | 80% | 656 / 888 | 74% |
| 12 | 752,261 | 750,736 | +0.20% | 25 / 33 | 76% | 661 / 887 | 75% |
| 16 | 674,890 | 677,167 | -0.34% | 12 / 12 | 100% | 536 / 782 | 69% |
| 19 | 662,516 | 661,923 | +0.09% | 5 / 7 | 71% | 505 / 751 | 67% |
| 22 | 662,135 | 661,855 | +0.04% | 4 / 6 | 67% | 506 / 745 | 68% |

### text-2m (2,000,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 740,784 | 741,684 | -0.12% | 356 / 384 | 93% | 1135 / 1158 | 98% |
| -1 | 670,742 | 670,894 | -0.02% | 336 / 363 | 93% | 1050 / 1085 | 97% |
| 1 | 601,693 | 601,830 | -0.02% | 328 / 353 | 93% | 1133 / 1198 | 95% |
| 2 | 602,780 | 602,850 | -0.01% | 326 / 362 | 90% | 1008 / 1024 | 98% |
| 3 | 569,551 | 569,150 | +0.07% | 314 / 321 | 98% | 1197 / 1209 | 99% |
| 4 | 566,147 | 565,958 | +0.03% | 319 / 327 | 98% | 1223 / 1250 | 98% |
| 5 | 572,870 | 573,488 | -0.11% | 131 / 134 | 98% | 1050 / 1066 | 98% |
| 6 | 559,736 | 560,444 | -0.13% | 82 / 85 | 96% | 1136 / 1158 | 98% |
| 7 | 550,766 | 550,823 | -0.01% | 60 / 74 | 81% | 1202 / 1243 | 97% |
| 8 | 549,591 | 549,737 | -0.03% | 43 / 54 | 80% | 1246 / 1310 | 95% |
| 9 | 549,435 | 549,586 | -0.03% | 43 / 55 | 78% | 1250 / 1305 | 96% |
| 12 | 523,825 | 524,029 | -0.04% | 19 / 24 | 79% | 1377 / 1450 | 95% |
| 16 | 462,165 | 461,886 | +0.06% | 5 / 5 | 100% | 1390 / 1492 | 93% |
| 19 | 459,178 | 459,028 | +0.03% | 3 / 4 | 75% | 1393 / 1478 | 94% |
| 22 | 459,178 | 459,028 | +0.03% | 4 / 4 | 100% | 1393 / 1489 | 94% |

### html-500k (500,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 132,355 | 132,678 | -0.24% | 578 / 637 | 91% | 1645 / 1852 | 89% |
| -1 | 118,954 | 118,970 | -0.01% | 522 / 574 | 91% | 1505 / 1682 | 89% |
| 1 | 105,976 | 105,973 | +0.00% | 512 / 554 | 92% | 1415 / 1715 | 83% |
| 2 | 103,838 | 103,845 | -0.01% | 482 / 523 | 92% | 1313 / 1529 | 86% |
| 3 | 98,779 | 98,697 | +0.08% | 430 / 471 | 91% | 1469 / 1616 | 91% |
| 4 | 98,610 | 98,535 | +0.08% | 443 / 476 | 93% | 1471 / 1644 | 89% |
| 5 | 93,700 | 93,549 | +0.16% | 205 / 208 | 99% | 1406 / 1594 | 88% |
| 6 | 91,615 | 91,705 | -0.10% | 135 / 142 | 95% | 1521 / 1652 | 92% |
| 7 | 90,558 | 90,585 | -0.03% | 103 / 122 | 84% | 1552 / 1706 | 91% |
| 8 | 90,903 | 90,892 | +0.01% | 75 / 91 | 82% | 1561 / 1789 | 87% |
| 9 | 90,903 | 90,892 | +0.01% | 75 / 91 | 82% | 1564 / 1778 | 88% |
| 12 | 89,261 | 89,240 | +0.02% | 41 / 49 | 84% | 1604 / 1871 | 86% |
| 16 | 82,840 | 82,794 | +0.06% | 9 / 9 | 100% | 1569 / 1702 | 92% |
| 19 | 82,352 | 82,310 | +0.05% | 4 / 5 | 80% | 1490 / 1679 | 89% |
| 22 | 82,352 | 82,310 | +0.05% | 4 / 5 | 80% | 1494 / 1677 | 89% |

### skewed-300k (300,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 100,518 | 100,313 | +0.20% | 383 / 418 | 92% | 829 / 934 | 89% |
| -1 | 90,759 | 90,545 | +0.24% | 360 / 392 | 92% | 758 / 877 | 86% |
| 1 | 80,717 | 80,700 | +0.02% | 342 / 393 | 87% | 887 / 989 | 90% |
| 2 | 82,935 | 82,824 | +0.13% | 337 / 371 | 91% | 744 / 848 | 88% |
| 3 | 80,185 | 80,152 | +0.04% | 333 / 353 | 94% | 958 / 1044 | 92% |
| 4 | 80,175 | 80,114 | +0.08% | 333 / 346 | 96% | 959 / 1041 | 92% |
| 5 | 80,422 | 80,404 | +0.02% | 135 / 128 | 105% | 819 / 926 | 88% |
| 6 | 76,713 | 76,715 | -0.00% | 78 / 74 | 105% | 926 / 1054 | 88% |
| 7 | 75,840 | 75,847 | -0.01% | 56 / 65 | 86% | 1038 / 1149 | 90% |
| 8 | 75,029 | 75,026 | +0.00% | 41 / 48 | 85% | 1068 / 1193 | 90% |
| 9 | 75,029 | 75,026 | +0.00% | 41 / 48 | 85% | 1067 / 1188 | 90% |
| 12 | 73,414 | 73,409 | +0.01% | 18 / 20 | 90% | 1221 / 1354 | 90% |
| 16 | 61,686 | 61,685 | +0.00% | 5 / 6 | 83% | 1401 / 2059 | 68% |
| 19 | 64,315 | 64,315 | +0.00% | 3 / 3 | 100% | 1341 / 1716 | 78% |
| 22 | 64,315 | 64,315 | +0.00% | 3 / 3 | 100% | 1341 / 1674 | 80% |

### random-100k (100,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 100,012 | 100,012 | +0.00% | 11823 / 15266 | 77% | 75019 / 52035 | 144% |
| -1 | 100,012 | 100,012 | +0.00% | 10959 / 13943 | 79% | 68587 / 47480 | 144% |
| 1 | 100,012 | 100,012 | +0.00% | 7868 / 10178 | 77% | 68587 / 48221 | 142% |
| 2 | 100,012 | 100,012 | +0.00% | 6667 / 9551 | 70% | 96061 / 47220 | 203% |
| 3 | 100,012 | 100,012 | +0.00% | 5556 / 7580 | 73% | 82781 / 42459 | 195% |
| 4 | 100,012 | 100,012 | +0.00% | 4436 / 7358 | 60% | 96061 / 44840 | 214% |
| 5 | 100,012 | 100,012 | +0.00% | 3625 / 4242 | 85% | 96061 / 39182 | 245% |
| 6 | 100,012 | 100,012 | +0.00% | 3593 / 4257 | 84% | 92336 / 47934 | 193% |
| 7 | 100,012 | 100,012 | +0.00% | 3609 / 4244 | 85% | 92251 / 41280 | 223% |
| 8 | 100,012 | 100,012 | +0.00% | 3371 / 4192 | 80% | 96061 / 43194 | 222% |
| 9 | 100,012 | 100,012 | +0.00% | 3179 / 3762 | 85% | 92336 / 40515 | 228% |
| 12 | 100,012 | 100,012 | +0.00% | 2479 / 763 | 325% | 96061 / 39778 | 241% |
| 16 | 100,012 | 100,012 | +0.00% | 53 / 45 | 118% | 68587 / 46587 | 147% |
| 19 | 100,012 | 100,012 | +0.00% | 27 / 23 | 117% | 68587 / 48129 | 143% |
| 22 | 100,012 | 100,012 | +0.00% | 27 / 23 | 117% | 95969 / 42649 | 225% |

## x86_64

The tables above are on Apple silicon. These are on an x86_64 Linux virtual machine with AVX2
and BMI2, one core per run. The reference is `zstd` 1.5.4; oxzstd is built with Rust 1.99.1
for the default x86-64 target, with its own release profile. Same benchmark, same method;
`html-4k` and `html-16k` are the benchmark's small inputs, the size of most web responses.

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
- Compression keeps close to the reference on HTML and generated text. The word list, with a
  sequence every 12 bytes or so, is where both compression (level 1: 66%) and decompression
  (65 to 80%) trail most: the per-sequence work is where the reference is still faster.
- Incompressible input is spotted early, as the reference does: up to level 9 oxzstd runs at
  75 to 85% of its speed, and from level 12 faster than it.
- The optimal parser's tables make levels 19 to 22 use more memory than the reference.
