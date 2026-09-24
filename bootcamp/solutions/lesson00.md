# Solutions — Lesson 0

[Back to Lesson 0](../lesson00_setup.md)

## 1. Seconds in a day

In the REPL:

```
fm> print 1 day in s
86400 s
fm> :quit
```

The same thing as a program:

```fermium
print 1 day in s
```

<!-- output -->
```
86400 s
```

## 2. `me.fm`

```fermium
# me.fm
print "Ada Lovelace"
print 1.75 m
```

<!-- output -->
```
Ada Lovelace
1.75 m
```

Run it with `fermium run me.fm` from the folder where you saved it.

## 3. Height in feet

```fermium
print 1.75 m in ft
```

<!-- output -->
```
5.74 ft
```

`in ft` converts the length to feet.

## 4. `fermium --help`

The command is `fermium check`. It checks the units of a program without running it:

```
fermium check me.fm
```

prints

```
me.fm: no problems found (units check out)
```
