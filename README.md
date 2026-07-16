# blit

```sh
cargo test --workspace
```

## metrics

`dev-v1.3.1` (`5beabed872`, slint) vs `blit` (`3909705eec`) on `armv7a-unknown-xous-elf`

### build time

| app         | build |   slint |    blit | improvement |
| ----------- | ----- | ------: | ------: | ----------: |
| launcher    | dev   | 47.72 s | 22.48 s |       52.9% |
| launcher    | prod  | 54.11 s | 26.76 s |       50.5% |
| lock screen | dev   | 37.49 s | 11.20 s |       70.1% |
| lock screen | prod  | 40.49 s | 12.21 s |       69.8% |

### production binary size

| app         |       slint |        blit | improvement |
| ----------- | ----------: | ----------: | ----------: |
| launcher    | 5,931,821 B | 3,527,141 B |       40.5% |
| lock screen | 4,428,589 B | 1,470,941 B |       66.8% |

### post-use app memory

| app         | slint ELF | slint app memory | slint total | blit ELF | blit app memory | blit total | improvement |
| ----------- | --------: | ---------------: | ----------: | -------: | --------------: | ---------: | ----------: |
| launcher    |  5,793 KB |           427 KB |    6,220 KB | 3,444 KB |          544 KB |   3,988 KB |       35.9% |
| lock screen |  4,325 KB |           239 KB |    4,564 KB | 1,436 KB |          704 KB |   2,140 KB |       53.1% |

app memory is total process memory minus allocated ELF sections

### render time

milliseconds per frame on Passport hardware

| case                            | blit mean | blit median | blit p95 | slint mean | slint median | slint p95 | speedup |
| ------------------------------- | --------: | ----------: | -------: | ---------: | -----------: | --------: | ------: |
| full-screen opaque repaint      |    13.484 |      13.306 |   14.114 |     21.439 |       21.301 |    22.522 |   1.60x |
| full-screen translucent repaint |    20.812 |      20.691 |   21.881 |     49.078 |       49.026 |    50.018 |   2.37x |
| 16 sparse tiles                 |     5.629 |       5.608 |    5.814 |     22.007 |       21.881 |    23.773 |   3.90x |
| 8 overlapping regions           |    23.463 |      23.285 |   24.536 |     54.419 |       54.260 |    56.335 |   2.33x |
| translucent cover               |    68.055 |      68.130 |   68.848 |    204.048 |      203.857 |   205.292 |   2.99x |
| opaque cover                    |    13.049 |      12.344 |   15.335 |    199.427 |      199.081 |   201.447 |  16.13x |
| scrolling images                |    13.504 |      13.412 |   14.191 |     40.369 |       40.344 |    40.985 |   3.01x |
| translating large image         |    20.938 |      20.325 |   26.001 |     49.801 |       49.683 |    51.117 |   2.44x |
| 240 small images                |     1.215 |       1.210 |    1.329 |     54.602 |       54.535 |    56.030 |  45.07x |
| wrapped paragraph               |     6.741 |       6.718 |    6.950 |     17.951 |       17.914 |    18.280 |   2.67x |
| 48 sparse labels                |     1.618 |       1.598 |    1.690 |     37.116 |       36.972 |    37.964 |  23.14x |
| changing short text             |     0.603 |       0.601 |    0.631 |      5.168 |        4.692 |     8.453 |   7.81x |
| rounded clipped list            |    15.388 |      15.297 |   15.915 |     44.433 |       44.388 |    45.105 |   2.90x |
| translating elevated card       |     9.708 |       9.689 |   10.040 |     31.361 |       31.219 |    32.562 |   3.22x |
| mixed stress                    |   117.686 |     117.569 |  119.507 |    351.215 |      345.642 |   389.832 |   2.94x |
