# Benchmarks

oxzstd against the reference C `zstd`, both single-threaded.

**Machine:** an Apple silicon (M1-class, arm64) Mac, one core per run. **Builds:** Rust
1.99.1, release profile (fat LTO, one codegen unit); `zstd` 1.5.7.

**Inputs:** `words` is the system word list (`/usr/share/dict/words`); the rest come from
the test corpus (`tests/common/mod.rs`), generated from fixed seeds: `text-2m` is
English-like text, `html-500k` HTML, `skewed-300k` three symbols with skewed frequencies,
`random-100k` random bytes.

**Speed and size:** `cargo bench --bench versus_zstd -- LEVELS [NAMES]`. Files in
`bench-data/` are added to the inputs (copy `words` there). oxzstd is timed through a reused
`Compressor` and `Decompressor` (into reused buffers), the best of repeated runs over about
half a second; the reference through `zstd -b -i1`. Each speed is the median of three rounds
(`--rounds=N` for more), taking oxzstd and the reference in turn so that both see the same
changes in the machine's speed: from one run to the next the reference's own speed can move by
10 to 20% on a busy machine. Neither side's frames carry a checksum
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
| -5 | -0.49% | 503 / 591 | 85% | 1169 / 1425 | 82% | 7.9 / 9.3 | 5.8 / 5.8 |
| -1 | -0.01% | 402 / 470 | 86% | 966 / 1147 | 84% | 7.7 / 9.0 | 5.5 / 5.5 |
| 1 | +0.04% | 394 / 486 | 81% | 864 / 1093 | 79% | 6.3 / 8.6 | 5.4 / 5.3 |
| 3 | +0.10% | 238 / 236 | 101% | 723 / 881 | 82% | 7.1 / 8.9 | 5.3 / 6.8 |
| 5 | +0.46% | 140 / 131 | 107% | 635 / 790 | 80% | 9.4 / 10.6 | 5.2 / 6.6 |
| 7 | +0.22% | 74.9 / 76.7 | 98% | 671 / 841 | 80% | 11.3 / 12.9 | 5.3 / 6.6 |
| 9 | +0.23% | 54.5 / 58.8 | 93% | 682 / 891 | 77% | 16.3 / 17.9 | 5.2 / 6.7 |
| 12 | +0.20% | 33.0 / 32.0 | 103% | 689 / 884 | 78% | 46.3 / 47.8 | 5.2 / 6.7 |
| 16 | -0.34% | 11.6 / 11.4 | 102% | 561 / 781 | 72% | 38.4 / 39.8 | 5.2 / 6.7 |
| 19 | +0.09% | 4.74 / 5.72 | 83% | 530 / 738 | 72% | 55.2 / 56.3 | 5.2 / 6.7 |
| 22 | +0.04% | 3.71 / 4.85 | 76% | 531 / 746 | 71% | 71.6 / 72.3 | 5.2 / 6.7 |

### html-500k

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd | Compress RAM MiB | Decompress RAM MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | -0.30% | 660 / 638 | 104% | 1752 / 1855 | 94% | 3.5 / 3.5 | 3.1 / 3.3 |
| -1 | -0.04% | 605 / 573 | 106% | 1603 / 1682 | 95% | 3.5 / 3.5 | 2.5 / 3.3 |
| 1 | -0.02% | 579 / 537 | 108% | 1506 / 1678 | 90% | 3.5 / 3.5 | 2.5 / 3.3 |
| 3 | +0.08% | 477 / 456 | 105% | 1553 / 1620 | 96% | 4.2 / 4.2 | 2.5 / 3.3 |
| 5 | +0.16% | 230 / 195 | 118% | 1494 / 1544 | 97% | 6.5 / 5.9 | 2.5 / 3.3 |
| 7 | -0.03% | 117 / 113 | 103% | 1640 / 1688 | 97% | 8.5 / 8.4 | 2.5 / 3.3 |
| 9 | +0.01% | 85.7 / 87.0 | 99% | 1664 / 1754 | 95% | 8.4 / 8.4 | 2.5 / 3.3 |
| 12 | +0.02% | 55.7 / 46.4 | 120% | 1723 / 1839 | 94% | 8.5 / 8.4 | 2.5 / 3.3 |
| 16 | +0.06% | 8.23 / 8.09 | 102% | 1672 / 1676 | 100% | 11.3 / 11.5 | 2.5 / 3.3 |
| 19 | +0.05% | 3.79 / 4.25 | 89% | 1593 / 1644 | 97% | 12.8 / 12.1 | 2.5 / 3.3 |
| 22 | +0.05% | 4.04 / 4.22 | 96% | 1598 / 1654 | 97% | 12.6 / 12.1 | 2.5 / 3.3 |

### text-2m

| Level | Size vs zstd | Compress MB/s | Compress vs zstd | Decompress MB/s | Decompress vs zstd | Compress RAM MiB | Decompress RAM MiB |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | -0.14% | 412 / 381 | 108% | 1179 / 1143 | 103% | 5.7 / 7.5 | 6.6 / 5.0 |
| -1 | -0.04% | 391 / 361 | 108% | 1109 / 1090 | 102% | 5.9 / 7.4 | 6.5 / 5.1 |
| 1 | -0.03% | 378 / 355 | 107% | 1171 / 1203 | 97% | 5.8 / 7.1 | 6.4 / 4.9 |
| 3 | +0.07% | 330 / 321 | 103% | 1255 / 1237 | 101% | 6.3 / 7.7 | 6.4 / 6.1 |
| 5 | -0.11% | 147 / 134 | 110% | 1085 / 1062 | 102% | 8.5 / 9.5 | 6.4 / 6.1 |
| 7 | -0.01% | 67.4 / 74.0 | 91% | 1243 / 1242 | 100% | 10.5 / 12.0 | 6.4 / 6.0 |
| 9 | -0.03% | 47.8 / 53.7 | 89% | 1288 / 1302 | 99% | 15.5 / 17.0 | 6.4 / 6.0 |
| 12 | -0.04% | 26.6 / 23.5 | 113% | 1425 / 1483 | 96% | 25.4 / 26.9 | 6.3 / 6.1 |
| 16 | +0.06% | 5.05 / 5.35 | 94% | 1452 / 1494 | 97% | 36.8 / 38.9 | 6.3 / 5.9 |
| 19 | +0.03% | 3.48 / 3.87 | 90% | 1448 / 1485 | 97% | 38.8 / 39.4 | 6.3 / 5.9 |
| 22 | +0.03% | 3.51 / 3.92 | 89% | 1448 / 1480 | 98% | 38.6 / 39.4 | 6.3 / 5.9 |

## Speed and size, every input

### words (2,493,885 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 1,428,912 | 1,435,949 | -0.49% | 503 / 591 | 85% | 1169 / 1425 | 82% |
| -1 | 1,127,878 | 1,127,996 | -0.01% | 402 / 470 | 86% | 966 / 1147 | 84% |
| 1 | 890,162 | 889,777 | +0.04% | 394 / 486 | 81% | 864 / 1093 | 79% |
| 2 | 863,682 | 863,511 | +0.02% | 306 / 293 | 105% | 776 / 957 | 81% |
| 3 | 853,674 | 852,806 | +0.10% | 238 / 236 | 101% | 723 / 881 | 82% |
| 4 | 855,293 | 854,288 | +0.12% | 237 / 230 | 103% | 719 / 864 | 83% |
| 5 | 802,409 | 798,716 | +0.46% | 140 / 131 | 107% | 635 / 790 | 80% |
| 6 | 770,771 | 767,493 | +0.43% | 93.3 / 89.2 | 105% | 663 / 844 | 79% |
| 7 | 770,232 | 768,540 | +0.22% | 74.9 / 76.7 | 98% | 671 / 841 | 80% |
| 8 | 749,333 | 747,610 | +0.23% | 57.1 / 58.7 | 97% | 681 / 884 | 77% |
| 9 | 749,924 | 748,233 | +0.23% | 54.5 / 58.8 | 93% | 682 / 891 | 77% |
| 10 | 751,283 | 749,713 | +0.21% | 42.0 / 43.5 | 97% | 687 / 891 | 77% |
| 11 | 752,046 | 750,514 | +0.20% | 35.1 / 33.3 | 105% | 688 / 892 | 77% |
| 12 | 752,261 | 750,736 | +0.20% | 33.0 / 32.0 | 103% | 689 / 884 | 78% |
| 13 | 742,222 | 740,238 | +0.27% | 24.0 / 22.3 | 108% | 670 / 875 | 77% |
| 14 | 741,974 | 740,017 | +0.26% | 20.7 / 19.2 | 108% | 671 / 878 | 76% |
| 15 | 741,954 | 740,002 | +0.26% | 19.2 / 17.9 | 107% | 670 / 874 | 77% |
| 16 | 674,893 | 677,167 | -0.34% | 11.6 / 11.4 | 102% | 561 / 781 | 72% |
| 17 | 668,053 | 668,048 | +0.00% | 8.72 / 8.59 | 102% | 548 / 755 | 73% |
| 18 | 663,120 | 662,194 | +0.14% | 5.34 / 6.11 | 87% | 531 / 743 | 72% |
| 19 | 662,516 | 661,923 | +0.09% | 4.74 / 5.72 | 83% | 530 / 738 | 72% |
| 20 | 662,516 | 661,922 | +0.09% | 5.00 / 5.49 | 91% | 530 / 740 | 72% |
| 21 | 662,516 | 661,922 | +0.09% | 4.37 / 5.57 | 78% | 530 / 743 | 71% |
| 22 | 662,135 | 661,855 | +0.04% | 3.71 / 4.85 | 76% | 531 / 746 | 71% |

### text-2m (2,000,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 740,681 | 741,684 | -0.14% | 412 / 381 | 108% | 1179 / 1143 | 103% |
| -1 | 670,645 | 670,894 | -0.04% | 391 / 361 | 108% | 1109 / 1090 | 102% |
| 1 | 601,675 | 601,830 | -0.03% | 378 / 355 | 107% | 1171 / 1203 | 97% |
| 2 | 602,682 | 602,850 | -0.03% | 378 / 358 | 106% | 1046 / 1032 | 101% |
| 3 | 569,546 | 569,150 | +0.07% | 330 / 321 | 103% | 1255 / 1237 | 101% |
| 4 | 566,181 | 565,958 | +0.04% | 336 / 326 | 103% | 1270 / 1246 | 102% |
| 5 | 572,858 | 573,488 | -0.11% | 147 / 134 | 110% | 1085 / 1062 | 102% |
| 6 | 559,758 | 560,444 | -0.12% | 92.1 / 85.7 | 107% | 1175 / 1165 | 101% |
| 7 | 550,766 | 550,823 | -0.01% | 67.4 / 74.0 | 91% | 1243 / 1242 | 100% |
| 8 | 549,589 | 549,737 | -0.03% | 48.9 / 54.2 | 90% | 1287 / 1311 | 98% |
| 9 | 549,435 | 549,586 | -0.03% | 47.8 / 53.7 | 89% | 1288 / 1302 | 99% |
| 10 | 536,752 | 536,867 | -0.02% | 36.4 / 34.3 | 106% | 1360 / 1401 | 97% |
| 11 | 523,830 | 524,029 | -0.04% | 26.7 / 23.5 | 113% | 1425 / 1480 | 96% |
| 12 | 523,830 | 524,029 | -0.04% | 26.6 / 23.5 | 113% | 1425 / 1483 | 96% |
| 13 | 511,992 | 512,026 | -0.01% | 10.1 / 9.85 | 102% | 1452 / 1535 | 95% |
| 14 | 496,206 | 496,294 | -0.02% | 7.20 / 7.06 | 102% | 1503 / 1647 | 91% |
| 15 | 496,039 | 496,184 | -0.03% | 6.50 / 6.04 | 108% | 1501 / 1649 | 91% |
| 16 | 462,173 | 461,886 | +0.06% | 5.05 / 5.35 | 94% | 1452 / 1494 | 97% |
| 17 | 463,794 | 463,419 | +0.08% | 4.89 / 5.00 | 98% | 1452 / 1496 | 97% |
| 18 | 459,921 | 459,584 | +0.07% | 3.58 / 4.04 | 89% | 1434 / 1484 | 97% |
| 19 | 459,178 | 459,028 | +0.03% | 3.48 / 3.87 | 90% | 1448 / 1485 | 97% |
| 20 | 459,178 | 459,028 | +0.03% | 3.49 / 3.94 | 89% | 1448 / 1488 | 97% |
| 21 | 459,178 | 459,028 | +0.03% | 3.49 / 3.86 | 90% | 1447 / 1479 | 98% |
| 22 | 459,178 | 459,028 | +0.03% | 3.51 / 3.92 | 89% | 1448 / 1480 | 98% |

### html-500k (500,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 132,277 | 132,678 | -0.30% | 660 / 638 | 104% | 1752 / 1855 | 94% |
| -1 | 118,925 | 118,970 | -0.04% | 605 / 573 | 106% | 1603 / 1682 | 95% |
| 1 | 105,954 | 105,973 | -0.02% | 579 / 537 | 108% | 1506 / 1678 | 90% |
| 2 | 103,825 | 103,845 | -0.02% | 543 / 514 | 106% | 1396 / 1506 | 93% |
| 3 | 98,779 | 98,697 | +0.08% | 477 / 456 | 105% | 1553 / 1620 | 96% |
| 4 | 98,610 | 98,535 | +0.08% | 476 / 462 | 103% | 1551 / 1620 | 96% |
| 5 | 93,700 | 93,549 | +0.16% | 230 / 195 | 118% | 1494 / 1544 | 97% |
| 6 | 91,615 | 91,705 | -0.10% | 152 / 136 | 112% | 1595 / 1661 | 96% |
| 7 | 90,558 | 90,585 | -0.03% | 117 / 113 | 103% | 1640 / 1688 | 97% |
| 8 | 90,903 | 90,892 | +0.01% | 85.8 / 88.5 | 97% | 1661 / 1766 | 94% |
| 9 | 90,903 | 90,892 | +0.01% | 85.7 / 87.0 | 99% | 1664 / 1754 | 95% |
| 10 | 89,946 | 89,935 | +0.01% | 68.9 / 61.7 | 112% | 1701 / 1802 | 94% |
| 11 | 89,261 | 89,240 | +0.02% | 56.0 / 46.7 | 120% | 1729 / 1843 | 94% |
| 12 | 89,261 | 89,240 | +0.02% | 55.7 / 46.4 | 120% | 1723 / 1839 | 94% |
| 13 | 88,749 | 88,721 | +0.03% | 23.5 / 22.3 | 105% | 1721 / 1838 | 94% |
| 14 | 88,281 | 88,262 | +0.02% | 20.2 / 18.8 | 108% | 1724 / 1865 | 92% |
| 15 | 88,282 | 88,262 | +0.02% | 19.4 / 17.5 | 111% | 1735 / 1863 | 93% |
| 16 | 82,840 | 82,794 | +0.06% | 8.23 / 8.09 | 102% | 1672 / 1676 | 100% |
| 17 | 82,817 | 82,782 | +0.04% | 7.60 / 6.96 | 109% | 1658 / 1653 | 100% |
| 18 | 82,524 | 82,471 | +0.06% | 4.97 / 5.46 | 91% | 1587 / 1616 | 98% |
| 19 | 82,352 | 82,310 | +0.05% | 3.79 / 4.25 | 89% | 1593 / 1644 | 97% |
| 20 | 82,352 | 82,310 | +0.05% | 3.83 / 4.40 | 87% | 1604 / 1653 | 97% |
| 21 | 82,352 | 82,310 | +0.05% | 3.76 / 4.27 | 88% | 1589 / 1647 | 97% |
| 22 | 82,352 | 82,310 | +0.05% | 4.04 / 4.22 | 96% | 1598 / 1654 | 97% |

### skewed-300k (300,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 100,492 | 100,313 | +0.18% | 443 / 416 | 107% | 870 / 932 | 93% |
| -1 | 90,734 | 90,545 | +0.21% | 415 / 390 | 106% | 808 / 873 | 93% |
| 1 | 80,711 | 80,700 | +0.01% | 423 / 395 | 107% | 936 / 997 | 94% |
| 2 | 82,911 | 82,824 | +0.11% | 388 / 369 | 105% | 785 / 852 | 92% |
| 3 | 80,194 | 80,152 | +0.05% | 358 / 353 | 101% | 1011 / 1054 | 96% |
| 4 | 80,088 | 80,114 | -0.03% | 355 / 352 | 101% | 1010 / 1049 | 96% |
| 5 | 80,422 | 80,404 | +0.02% | 153 / 127 | 120% | 863 / 932 | 93% |
| 6 | 76,713 | 76,715 | -0.00% | 89.9 / 75.3 | 119% | 981 / 1056 | 93% |
| 7 | 75,840 | 75,847 | -0.01% | 63.5 / 65.0 | 98% | 1082 / 1142 | 95% |
| 8 | 75,029 | 75,026 | +0.00% | 45.9 / 47.8 | 96% | 1115 / 1202 | 93% |
| 9 | 75,029 | 75,026 | +0.00% | 45.8 / 47.8 | 96% | 1114 / 1202 | 93% |
| 10 | 74,138 | 74,145 | -0.01% | 34.1 / 31.7 | 108% | 1210 / 1291 | 94% |
| 11 | 73,414 | 73,409 | +0.01% | 24.2 / 20.5 | 118% | 1279 / 1364 | 94% |
| 12 | 73,414 | 73,409 | +0.01% | 24.3 / 20.4 | 119% | 1279 / 1363 | 94% |
| 13 | 72,736 | 72,731 | +0.01% | 11.9 / 12.0 | 99% | 1310 / 1395 | 94% |
| 14 | 70,560 | 70,554 | +0.01% | 8.02 / 7.96 | 101% | 1392 / 1552 | 90% |
| 15 | 70,107 | 70,099 | +0.01% | 7.38 / 7.24 | 102% | 1394 / 1564 | 89% |
| 16 | 61,686 | 61,685 | +0.00% | 5.03 / 5.57 | 90% | 1401 / 2046 | 68% |
| 17 | 61,844 | 61,843 | +0.00% | 4.56 / 5.07 | 90% | 1409 / 2070 | 68% |
| 18 | 63,434 | 63,434 | +0.00% | 3.51 / 4.09 | 86% | 1389 / 1807 | 77% |
| 19 | 64,315 | 64,315 | +0.00% | 2.51 / 2.96 | 85% | 1398 / 1709 | 82% |
| 20 | 64,315 | 64,315 | +0.00% | 2.51 / 2.95 | 85% | 1397 / 1704 | 82% |
| 21 | 64,315 | 64,315 | +0.00% | 2.51 / 2.98 | 84% | 1395 / 1704 | 82% |
| 22 | 64,315 | 64,315 | +0.00% | 2.50 / 2.88 | 87% | 1399 / 1702 | 82% |

### random-100k (100,000 bytes)

| Level | oxzstd bytes | zstd bytes | Size vs zstd | Compress MB/s, oxzstd / zstd | Compress vs zstd | Decompress MB/s, oxzstd / zstd | Decompress vs zstd |
|---:|---:|---:|---:|---:|---:|---:|---:|
| -5 | 100,012 | 100,012 | +0.00% | 14815 / 15022 | 99% | 68587 / 29975 | 229% |
| -1 | 100,012 | 100,012 | +0.00% | 13714 / 14131 | 97% | 68587 / 34307 | 200% |
| 1 | 100,012 | 100,012 | +0.00% | 9160 / 10141 | 90% | 68587 / 34339 | 200% |
| 2 | 100,012 | 100,012 | +0.00% | 8511 / 9300 | 92% | 68587 / 39851 | 172% |
| 3 | 100,012 | 100,012 | +0.00% | 5970 / 7498 | 80% | 70572 / 29815 | 237% |
| 4 | 100,012 | 100,012 | +0.00% | 4888 / 7379 | 66% | 68587 / 32281 | 212% |
| 5 | 100,012 | 100,012 | +0.00% | 4075 / 4246 | 96% | 68587 / 32062 | 214% |
| 6 | 100,012 | 100,012 | +0.00% | 4007 / 4253 | 94% | 68587 / 44578 | 154% |
| 7 | 100,012 | 100,012 | +0.00% | 3993 / 4255 | 94% | 68587 / 35441 | 194% |
| 8 | 100,012 | 100,012 | +0.00% | 3768 / 4241 | 89% | 68587 / 35243 | 195% |
| 9 | 100,012 | 100,012 | +0.00% | 3762 / 3770 | 100% | 68587 / 37078 | 185% |
| 10 | 100,012 | 100,012 | +0.00% | 3604 / 3239 | 111% | 68587 / 33669 | 204% |
| 11 | 100,012 | 100,012 | +0.00% | 762 / 764 | 100% | 68587 / 34147 | 201% |
| 12 | 100,012 | 100,012 | +0.00% | 741 / 763 | 97% | 68587 / 35173 | 195% |
| 13 | 100,012 | 100,012 | +0.00% | 60.0 / 67.0 | 90% | 70621 / 30224 | 234% |
| 14 | 100,012 | 100,012 | +0.00% | 52.9 / 45.6 | 116% | 68587 / 33470 | 205% |
| 15 | 100,012 | 100,012 | +0.00% | 53.0 / 45.8 | 116% | 68587 / 28429 | 241% |
| 16 | 100,012 | 100,012 | +0.00% | 53.2 / 45.0 | 118% | 68587 / 31047 | 221% |
| 17 | 100,012 | 100,012 | +0.00% | 53.2 / 45.0 | 118% | 68587 / 32087 | 214% |
| 18 | 100,012 | 100,012 | +0.00% | 53.1 / 44.8 | 118% | 68587 / 58192 | 118% |
| 19 | 100,012 | 100,012 | +0.00% | 26.7 / 22.9 | 117% | 68587 / 47020 | 146% |
| 20 | 100,012 | 100,012 | +0.00% | 26.8 / 22.7 | 118% | 68587 / 51223 | 134% |
| 21 | 100,012 | 100,012 | +0.00% | 26.8 / 23.0 | 116% | 68587 / 32138 | 213% |
| 22 | 100,012 | 100,012 | +0.00% | 26.8 / 22.9 | 117% | 68587 / 30226 | 227% |

## x86_64

The tables above are on Apple silicon. These are on an x86_64 Linux virtual machine (QEMU,
AVX2 and BMI2), one core per run, in a Debian container. The reference is Debian's `zstd`
1.5.7; oxzstd is built with Rust 1.99.1 for the default x86-64 target, with its own release
profile. Same benchmark; `html-4k` and `html-16k` are the benchmark's small inputs, the size
of most web responses. These were measured before the row search's speedups at levels 7 to
12, and before the benchmark took the median of several rounds and gave slow speeds to three
digits (here they're whole MB/s).

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
- Compression keeps close to the reference on HTML and generated text (103 to 108% at levels
  1 and 3). The word list, with a sequence every 12 bytes or so, is where the fast levels
  (level 1: 81%) and decompression (71 to 84%) trail most: the per-sequence work is where the
  reference is still faster.
- The lazy levels' row search (levels 7 to 12) runs at 89 to 103% of the reference's speed
  at levels 7 to 9, and 97 to 120% at levels 10 to 12; the binary tree (13 to 15) at 99 to
  111%.
- The optimal parser is behind at its highest levels: 76 to 96% at levels 18 to 22 (the word
  list least, 76 to 91%), against 90 to 109% at 16 and 17.
- Decompression of text and HTML runs at 90 to 103% of the reference's speed. The skewed
  input at levels 16 to 22, made almost entirely of matches only a few bytes back, decodes at
  68 to 82%.
- Incompressible input is spotted early, as the reference does: up to level 13 oxzstd runs at
  66 to 111% of its speed, and from level 14 faster than it.
- At levels 19 to 22 the optimal parser's tables take about as much memory as the
  reference's: 0.5 MiB more on HTML, about 1 MiB less on text and the word list.
