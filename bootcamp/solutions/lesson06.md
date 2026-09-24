# Solutions — Lesson 6

[Back to Lesson 6](../lesson06_data_plotting.md)

These programs load files from `data/`, so they are meant to be saved in the `bootcamp` folder. (Here in `solutions/`, the paths start with `../data/`, which means "go up one folder, then into `data`".)

## 1. Your own data

The file `data/spring.csv`:

```
M [kg], x [cm]
0.1, 1.9
0.2, 4.1
0.3, 5.9
0.4, 8.2
0.5, 9.9
```

```fermium
s = load "../data/spring.csv"
print s.M
print s.x
```

<!-- output -->
```
[0.1, 0.2, 0.3, 0.4, 0.5] kg
[1.9, 4.1, 5.9, 8.2, 9.9] cm
```

## 2. Spring constant

```fermium
s = load "../data/spring.csv"
fit x = M * 9.81 m/s^2 / k to s
print k in N/m
plot s.x vs s.M to "spring_data.png"
```

<!-- output -->
```
fit x = M·9.81 m/s^2/k   (5 data points from ../data/spring.csv)
  k = 49.01 N/m   (standard error 0.47 N/m)
  rms residual = 0.00126 m
49.0 N/m
plot saved to spring_data.png
```

About 49 N/m: a 1 kg mass would stretch this spring by about 20 cm.

## 3. Pendulum statistics

```fermium
data = load "../data/pendulum.csv"
g = 4 pi^2 data.L / data.T^2
deviation = abs(g - 9.81 m/s^2)
worst = max(deviation)
for i from 1 to len(g)
    if deviation[i] == worst
        print "row", i, ": L =", data.L[i], "g =", g[i]
```

<!-- output -->
```
row 2 : L = 0.4 m g = 9.63829 m/s²
```

## 4. Decay rate

```fermium
d = load "../data/decay.csv"
fit counts = N0 exp(-t / tau) to d with N0 = 1000, tau = 10 min
decay_constant = 1 / tau
print decay_constant in 1/s
print decay_constant in 1/min
print "activity at t = 0:", decay_constant N0 in 1/min
```

<!-- output -->
```
fit counts = N0 exp(-t/τ)   (13 data points from ../data/decay.csv)
  N0 = 1196   (standard error 9.4)
  τ = 20.10 min   (standard error 0.26 min)
  rms residual = 10.8
0.000829 1/s
0.0498 1/min
activity at t = 0: 59.5 1/min
```

(We called it `decay_constant` rather than `lambda`, which would be the Greek letter λ, also fine.)

## 5. Plot a model

```fermium
d = load "../data/decay.csv"
ts = linspace(0 min, 60 min, 61)
model = 1200 exp(-ts / 20 min)
plot d.counts vs d.t, model vs ts to "decay_model.png"
```

<!-- output -->
```
plot saved to decay_model.png
```
