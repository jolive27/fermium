# Lesson 0 — Setting up your Mac

In this lesson you will install Fermium and run your first program. Nothing here is hard, but there are a lot of small steps. Go slowly, and read each step to the end before doing it.

**What you need:** a Mac (any Mac from the last ~8 years), an internet connection, and about 30 minutes.

---

## Step 1: Open the Terminal

The **Terminal** is a window where you type commands to your computer instead of clicking. Programmers use it all the time. It looks old-fashioned, but it's just a place to type.

1. Press **⌘ Command + Space** to open Spotlight (the search bar).
2. Type `Terminal` and press **Return**.

A window opens with a line that ends in `%` (or `$`), something like:

```
yourname@MacBook ~ %
```

This is the **prompt**. It means "I'm waiting for you to type a command". In this course, when you see a grey box like the one below, type what's inside it after the prompt and press **Return**:

```
echo hello
```

The computer answers `hello`. You just ran your first command.

> **Tip:** In the Terminal you can't click to move the cursor. Use the ← and → arrow keys. The ↑ key brings back the previous command, which saves a lot of typing.

## Step 2: Install Python 3

Fermium is written in **Python**, another programming language, so your Mac needs Python version **3.10 or newer**.

Check what you have. Type:

```
python3 --version
```

- If it prints `Python 3.10.x`, `3.11.x`, `3.12.x`, `3.13.x` or newer: great, skip to Step 3.
- If it prints `Python 3.9.6` (or anything below 3.10), or a window pops up offering to install "command line developer tools", or you get `command not found`: you need to install a newer Python. (The Python that comes with macOS is too old for Fermium. You can close that pop-up window.)

**To install Python:**

1. Open **https://www.python.org/downloads/** in Safari.
2. Click the big yellow **Download Python 3.x.x** button. This downloads a file ending in `.pkg`.
3. Open the downloaded file (it's in your Downloads folder) and click **Continue** / **Agree** / **Install** through the installer. It will ask for your Mac password.
4. When it finishes, **quit the Terminal completely** (⌘ Command + Q) and open it again. This matters: a Terminal that was open before the install doesn't know about the new Python.
5. Check again:

```
python3 --version
```

It should now say something like `Python 3.13.1`.

## Step 3: Get the Fermium folder

Fermium lives in a folder called `fermium`. Put it in your **home folder** (the folder with the little house icon in Finder, named after you).

- If you received it as a **ZIP file** (for example from GitHub's green **Code → Download ZIP** button): double-click the ZIP to unpack it, rename the folder to `fermium` if it is called something like `fermium-main`, and drag it into your home folder.
- If you know how to use `git`, you can clone it instead.

Now tell the Terminal to go into that folder. `cd` means "change directory" (a *directory* is the same thing as a folder):

```
cd ~/fermium
```

The `~` is short for your home folder. Check you're in the right place with `ls` ("list"), which shows the files in the current folder:

```
ls
```

You should see names like `bootcamp`, `fermium`, `pyproject.toml` and `README.md`.

## Step 4: Install Fermium

Still in the `fermium` folder, type this (all one line, including the dot and the quotes):

```
python3 -m pip install -e ".[full]"
```

What this means, piece by piece:
- `python3 -m pip` runs **pip**, Python's tool for installing software.
- `install -e .` installs the program in the current folder (`.` means "this folder"). The `-e` means "editable": if the folder is updated later, you don't need to reinstall.
- `[full]` also installs the optional extras Fermium uses for fitting data, symbolic integrals and plots (SciPy, SymPy and Matplotlib).

You'll see a lot of text scroll by while it downloads things. That's normal. It should end with a line starting with `Successfully installed ...`.

> **In the future:** once Fermium is published, the whole of Step 3 and Step 4 will be one line: `python3 -m pip install "fermium[full]"`.

## Step 5: Check the installation with `fermium doctor`

Fermium comes with a checkup command:

```
fermium doctor
```

You should see something like this (your version numbers will differ):

```
Checking your Fermium installation...

  ✓ Python 3.11.15 (Linux x86_64)
  ✓ llvmlite 0.49.0 (LLVM 22.1.0)
  ✓ numpy 2.4.6
  ✓ scipy 1.17.1
  ✓ sympy 1.14.0
  ✓ matplotlib 3.11.2
  ✓ compiled and ran a test program: g = 9.70 m/s²

Everything looks good! Try:  fermium   (then type  print 2 m + 30 cm )
```

(On a Mac the first line says `Darwin arm64` or `Darwin x86_64` instead of `Linux`. *Darwin* is the technical name of macOS.)

A `✗` means something is missing. The line below it, starting with `fix:`, tells you what to type. Do that, then run `fermium doctor` again.

## Step 6: Try the interactive prompt

Type `fermium` on its own and press Return:

```
fermium
```

You're now *inside* Fermium. The prompt changes to `fm>`. Type a line and press Return; Fermium answers straight away:

```
fm> print 2 m + 30 cm
2.3 m
fm> print 1 mi in km
1.60934 km
```

It added 2 metres and 30 centimetres correctly! This interactive prompt is called the **REPL** (Read–Evaluate–Print Loop: it reads a line, works it out, prints the answer, and repeats). It's perfect for quick calculations.

To leave the REPL and go back to the normal Terminal, type `:quit` (or press **Control + D**).

## Step 7: Your first program in a file

For anything longer than a line or two, you write a **program**: a text file with instructions, which Fermium runs from top to bottom. Fermium programs end in `.fm`.

### Get a code editor

You need a **text editor** made for code. We recommend **Visual Studio Code** ("VS Code"), which is free:

1. Download it from **https://code.visualstudio.com** and drag it into your Applications folder.
2. Open VS Code. Choose **File → Open Folder…** and open your `fermium` folder.
3. *(Optional but nice)* Install the Fermium extension for VS Code. It colours your code and lets you type `\theta` then Tab to get `θ`. It lives in the folder `editors/vscode/` inside `fermium`. Open VS Code once (so it creates its settings folder), then type this in the Terminal (one line) and restart VS Code:

   ```
   ln -s ~/fermium/editors/vscode ~/.vscode/extensions/fermium
   ```

   (`ln -s` makes a *shortcut*, which Mac people call an alias, so VS Code finds the extension inside your `fermium` folder. More details are in `editors/vscode/README.md`.)

> **Don't use TextEdit** (the Mac's built-in editor) for code: it likes to turn `"` into curly quotes `“ ”` and save files as "rich text", which Fermium can't read. If you must use it, choose **Format → Make Plain Text** first.

### Write the program

1. In VS Code choose **File → New File…**, and save it (⌘ S) inside your `fermium` folder as `hello.fm`.
2. Type this into it and save again:

```fermium
# My first Fermium program
print "Hello, physics!"
print 9.81 m/s^2 * 3 s
```

3. Go back to the Terminal (make sure you're in the `fermium` folder: `cd ~/fermium`) and run it:

```
fermium run hello.fm
```

It prints:

```
Hello, physics!
29.4 m/s
```

The first line starting with `#` is a **comment**: a note for humans that Fermium ignores. `print` shows something on the screen. Text in quotes is printed as it is. `9.81 m/s^2 * 3 s` is a calculation: an acceleration times a time is a speed, and Fermium worked out the units for you.

> **Tip:** VS Code has its own Terminal built in: **View → Terminal**. You can edit and run in the same window.

## When something goes wrong

Here is what to do, in order:

1. **Read the message.** Fermium's error messages name the line, point at the problem with `^^^`, and often add a `hint:`.
2. **Run `fermium doctor`.** It checks everything and tells you what to fix.
3. **Look in [TROUBLESHOOTING.md](TROUBLESHOOTING.md).** It lists the most common problems.

If a program seems stuck (nothing happens and the prompt doesn't come back), press **Control + C**. Fermium stops the program and prints `stopped by Ctrl+C`.

The most common setup problems:

| What you see | What it means | What to do |
|---|---|---|
| `zsh: command not found: fermium` | The install didn't finish, or the Terminal hasn't noticed it yet | Quit and reopen the Terminal. If that doesn't help, repeat Step 4 and read the last lines it prints. |
| `zsh: command not found: python3` or a pop-up about "developer tools" | Python isn't installed | Do Step 2. |
| `ERROR: ... does not appear to be a Python project` | You ran the install from the wrong folder | `cd ~/fermium`, check with `ls` that you see `pyproject.toml`, and try again. |
| `requires a different Python: 3.9.6 not in '>=3.10'` | `python3` is still the old Apple Python | Quit and reopen the Terminal after installing Python from python.org. `python3 --version` must say 3.10 or more. |
| `can't find the file 'hello.fm'` | The Terminal is in a different folder from your file (or the name is spelled differently) | `cd` into the folder where you saved the file, and `ls` to check the file is there. |
| `fermium doctor` shows `✗ matplotlib is not installed` | The optional extras are missing | `python3 -m pip install matplotlib` (or redo Step 4 with `".[full]"`). |

If pip warns that a script was installed in a folder "which is not on PATH", your Terminal can't find the `fermium` command. The simplest fix is to reinstall Python from python.org (Step 2), quit the Terminal, reopen it, and redo Step 4.

## Exercises

1. Open the REPL (`fermium`) and work out how many seconds there are in a day (`print 1 day in s`). Leave with `:quit`.
2. Make a program `me.fm` that prints your name and your height in metres, e.g. `print 1.75 m`. Run it.
3. In the REPL, what does `print 1.75 m in ft` show? (You'll learn how this works in Lesson 1.)
4. Run `fermium --help` and read the list of commands. Which command checks a program's units without running it?

Solutions: [solutions/lesson00.md](solutions/lesson00.md)

**Next:** [Lesson 1 — Numbers & units](lesson01_numbers_units.md)
