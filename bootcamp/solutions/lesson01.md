# Solutions — Lesson 1

[Back to Lesson 1](../lesson01_numbers_units.md)

## 1. Light from the Sun

```fermium
print 1 AU / c in s
print 1 AU / c in min
```

<!-- output -->
```
499 s
8.32 min
```

About 8 minutes and 19 seconds. When you look at the Sun, you see it as it was 8 minutes ago.

## 2. Speed limits

```fermium
print 70 mph in km/hr
print 70 mph in m/s
```

<!-- output -->
```
113 km/hr
31.3 m/s
```

## 3. Rest energies

```fermium
print m_n * c^2 in MeV
print (m_n - m_p) * c^2 in MeV
```

<!-- output -->
```
940 MeV
1.29 MeV
```

The neutron is about 1.29 MeV heavier than the proton. That's why a free neutron can decay into a proton (plus an electron and an antineutrino).

## 4. Spot the bug

`3 m/s^2` is an *acceleration*, not a speed, and the speed isn't squared. Fermium prints:

```fermium
print 0.5 * 2 kg * 3 m/s^2
```

<!-- output -->
```
3.0 N
```

newtons, so it's a force, not an energy. Units told us something was wrong. The fix is to square the speed: `(3 m/s)^2`.

```fermium
print 0.5 * 2 kg * (3 m/s)^2
```

<!-- output -->
```
9.0 J
```

## 5. Photon energy

```fermium
print h * c / 530 nm
print h * c / 530 nm in eV
```

<!-- output -->
```
3.75×10⁻¹⁹ J
2.34 eV
```

About 2.3 eV, typical for visible light.
