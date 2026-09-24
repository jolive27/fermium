# Solutions — Lesson 4

[Back to Lesson 4](../lesson04_conditions_loops.md)

## 1. Grade converter

```fermium
score = 73
if score >= 70
    print "A"
else if score >= 60
    print "B"
else if score >= 50
    print "C"
else
    print "fail"
```

<!-- output -->
```
A
```

The order matters: we check the highest grade first, so a 73 doesn't also count as a "B".

## 2. Sum of squares

```fermium
total = 0
for n from 1 to 10
    total += n^2
print total
n = 10
print n (n + 1) (2 n + 1) / 6
```

<!-- output -->
```
385
385
```

## 3. Unit table

```fermium
for d from 0 km to 10 km step 1 km
    print d, d / c in us
```

<!-- output -->
```
0 km 0 μs
1 km 3.33564 μs
2 km 6.67128 μs
3 km 10.0069 μs
4 km 13.3426 μs
5 km 16.6782 μs
6 km 20.0138 μs
7 km 23.3495 μs
8 km 26.6851 μs
9 km 30.0208 μs
10 km 33.3564 μs
```

About 3.3 μs per kilometre, which is why engineers say "light goes about a foot per nanosecond".

## 4. Doubling

```fermium
N = 1
t = 0 min
while N <= 1000000
    N = 2 * N
    t += 20 min
print N, "bacteria after", t, "=", t in hr
```

<!-- output -->
```
1.04858×10⁶ bacteria after 400 min = 6.66667 hr
```

`2 * N`, not `2 N`: `2 N` would be two newtons! (Fermium would stop with an error, because `N` holds a plain number.)

## 5. Air resistance

```fermium
g = 9.81 m/s^2
k = 0.01 1/m        # drag constant
dt = 0.001 s
y = 0 m
v = 20 m/s
t = 0 s
y_max = 0 m

while y >= 0 m
    y += v dt
    v -= g dt + k v abs(v) dt
    t += dt
    if y > y_max
        y_max = y

print "flight time:", t to 4 digits
print "highest point:", y_max to 4 digits
```

<!-- output -->
```
flight time: 3.736 s
highest point: 17.11 m
```

Compared with 4.08 s and 20.4 m without air: the ball goes about 15% less high and lands sooner. `v abs(v)` (rather than `v^2`) makes the drag always point against the motion, both on the way up and on the way down.
