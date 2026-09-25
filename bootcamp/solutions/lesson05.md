# Solutions — Lesson 5

[Back to Lesson 5](../lesson05_lists.md)

## 1. Lab statistics

```fermium
g = [9.78 m/s^2, 9.83 m/s^2, 9.81 m/s^2, 9.75 m/s^2, 9.86 m/s^2]
print "mean:", mean(g)
print "std dev:", std(g)
print "std error:", std(g) / sqrt(len(g))
```

<!-- output -->
```
mean: 9.81 m/s²
std dev: 0.0428 m/s²
std error: 0.0191 m/s²
```

## 2. Free-fall table

```fermium
t = linspace(0 s, 1 s, 11)
d = ½ * 9.81 m/s^2 * t^2
print t
print d
```

<!-- output -->
```
[0, 0.100, 0.200, 0.300, 0.400, 0.500, 0.600, 0.700, 0.800, 0.900, 1.00] s
[0, 0.0491, 0.196, 0.441, 0.785, 1.23, 1.77, 2.40, 3.14, 3.97, 4.91] m
```

## 3. Kepler again

T²/a³ = 1 yr²/AU³, so T = √(a³ · 1 yr²/AU³):

```fermium
a = 9.537 AU
T = sqrt(a^3 * 1 yr^2/AU^3)
print T in yr
```

<!-- output -->
```
29.45 yr
```

The real value is 29.45 yr: agreement to 0.1%.

## 4. Building a list

```fermium
powers = []
p = 1
for n from 1 to 10
    p = 2 p
    push(powers, p)
print powers
print sum(powers)
```

<!-- output -->
```
[2, 4, 8, 16, 32, 64, 128, 256, 512, 1024]
2046
```

(The sum is 2¹¹ − 2 = 2046.)

## 5. Find the maximum

```fermium
xs = [3.2 m, 7.1 m, 1.4 m, 6.9 m]
largest = xs[1]
for x in xs
    if x > largest
        largest = x
print largest
print max(xs)
```

<!-- output -->
```
7.1 m
7.1 m
```
