# Solutions — Lesson 2b

[Back to Lesson 2b](../lesson02b_symbols.md)

## 1. Translate to ASCII

`½` becomes `(1/2)` and `v²` becomes `v^2`:

```fermium
E = (1/2) m v^2 where m = 2 kg, v = 3 m/s
print E
```

<!-- output -->
```
9 J
```

## 2. Translate to symbols

```fermium
ω₀ = √(k / m) where k = 50 N/m, m = 0.5 kg
print ω₀ in rad/s
```

<!-- output -->
```
10 rad/s
```

## 3. Use the formatter

`fermium fmt ke.fm --pretty` prints

```
E = ½ m v² where m = 2 kg, v = 3 m/s
print E
```

followed by the note `formatted ke.fm (not run — use fermium run)`. `-w` writes that into the file, and `fermium fmt ke.fm --ascii` turns it back into `E = (1/2) m v^2 ...`. Every version prints `9 J`.

## 4. Tab completion

Type `print 2\pi` Tab ` \sqrt` Tab `(1.0 m / 9.81 m/s\^2` Tab `)`:

```fermium
print 2π √(1.0 m / 9.81 m/s²)
```

<!-- output -->
```
2.0 s
```

It's the period of a 1 m pendulum: T = 2π√(L/g).

## 5. Two names, one variable

Yes, it works: `theta` and `θ` are two spellings of the same name.

```fermium
theta = 30 deg
print sin(θ)
```

<!-- output -->
```
0.500
```
