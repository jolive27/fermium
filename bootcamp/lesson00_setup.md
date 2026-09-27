# Lesson 0 — Setting up your computer

In this lesson you will install Fermium and run your first program. Nothing here is hard, but there are a lot of small steps. Go slowly, and read each step to the end before doing it.

**What you need:** an internet connection, about 20 minutes, and one of these computers:

| Your computer | What to download |
|---|---|
| A Mac with Apple silicon (M1, M2, M3, M4…: most Macs sold since late 2020) | `fermium-macos-arm64` |
| A Linux PC (Intel or AMD, "x86_64") | `fermium-linux-x86_64` |
| A Mac with an Intel processor | not built yet: [build it from the source](#building-fermium-from-the-source) |
| Windows | not built yet. **WSL** works: install Ubuntu from the Microsoft Store (search "WSL"), open it, and follow the Linux instructions inside it. |

Not sure which Mac you have? Click the Apple menu  → **About This Mac**. If the line **Chip** says *Apple M…*, you have Apple silicon; if it says **Processor** … *Intel*, you have an Intel Mac.

Fermium is **one file**: a single program with everything inside it (the compiler, the units, the calculus, the plots). You don't need to install Python, a C compiler or anything else to use it.

---

## Step 1: Open the Terminal

The **Terminal** is a window where you type commands to your computer instead of clicking. Programmers use it all the time. It looks old-fashioned, but it's just a place to type.

- **Mac:** press **⌘ Command + Space** to open Spotlight (the search bar), type `Terminal` and press **Return**.
- **Linux:** press **Ctrl + Alt + T** (on most Linux systems), or look for *Terminal* in your applications.

A window opens with a line that ends in `%` or `$`, something like:

```
yourname@MacBook ~ %
```

This is the **prompt**. It means "I'm waiting for you to type a command". In this course, when you see a grey box like the one below, type what's inside it after the prompt and press **Return** (or **Enter**):

```
echo hello
```

The computer answers `hello`. You just ran your first command.

> **Tip:** In the Terminal you can't click to move the cursor. Use the ← and → arrow keys. The ↑ key brings back the previous command, which saves a lot of typing. You can also copy a command from this page and paste it into the Terminal (**⌘ V** on a Mac, **Ctrl + Shift + V** on Linux).

## Step 2: Download Fermium

> **Not published yet:** the first release (v2.0) hasn't been uploaded to the Releases page yet. Until it is, build Fermium from the source ([Building Fermium from the source](#building-fermium-from-the-source), below) and continue at Step 4, or use the Python version ([below](#the-old-python-version-fermium-15-deprecated)).

1. Open the Fermium **Releases** page in your web browser: **https://github.com/jolive27/fermium/releases**
2. Under the newest release, click **Assets**, then click the file for your computer (see the table at the top): `fermium-macos-arm64` or `fermium-linux-x86_64`.

Your browser saves it in your **Downloads** folder. That one file is the whole of Fermium.

## Step 3: Put it on your PATH

When you type a command such as `fermium`, the Terminal looks for a program with that name in a short list of folders. That list is called your **PATH**. To make `fermium` work from anywhere, we'll make a folder for your own programs, move Fermium into it, and add that folder to your PATH. Do it once and you never have to think about it again.

**3a. Make a folder called `bin` in your home folder** (*bin*, short for "binaries", is the traditional name for a folder of programs; `~` is short for your home folder, the one named after you):

```
mkdir -p ~/bin
```

**3b. Move the downloaded file there and name it `fermium`.** Type the line for your computer:

On a Mac:

```
mv ~/Downloads/fermium-macos-arm64 ~/bin/fermium
```

On Linux:

```
mv ~/Downloads/fermium-linux-x86_64 ~/bin/fermium
```

(`mv` means "move". Giving the new place a different name renames the file at the same time.)

**3c. Allow the file to run as a program.** Browsers never mark a download as a program, for safety, so you tell the computer it's allowed (`chmod +x` means "make executable"):

```
chmod +x ~/bin/fermium
```

**3d. Mac only: tell macOS you trust it.** macOS protects you from programs downloaded from the internet with a feature called **Gatekeeper**: it marks each download as "quarantined", and a quarantined program that doesn't come from the App Store is blocked with a message like *"fermium" cannot be opened because the developer cannot be verified*. This command removes the quarantine mark from Fermium (and only from Fermium):

```
xattr -d com.apple.quarantine ~/bin/fermium
```

If it answers `No such xattr: com.apple.quarantine`, the file wasn't quarantined, which is fine.

**3e. Add `~/bin` to your PATH.** Your PATH is set by a small settings file that the Terminal reads every time it opens. This command adds one line to the end of that file:

On a Mac (the Terminal's shell is called *zsh*, and its settings file is `~/.zshrc`):

```
echo 'export PATH="$HOME/bin:$PATH"' >> ~/.zshrc
```

On Linux (the shell is usually *bash*, and its settings file is `~/.bashrc`):

```
echo 'export PATH="$HOME/bin:$PATH"' >> ~/.bashrc
```

Type it exactly, with the single quotes `'` on the outside and the double quotes `"` inside. The `>>` means "add to the end of this file". Then **close the Terminal window and open a new one**: only new windows read the settings file.

**3f. Check it worked.** In the new window, type:

```
fermium --version
```

It prints the version, for example `fermium 2.0.0 (Rust)`. If instead it says `command not found: fermium`, see [When something goes wrong](#when-something-goes-wrong).

## Step 4: Check the installation with `fermium doctor`

Fermium comes with a checkup command:

```
fermium doctor
```

You should see something like this (your version numbers, folder and platform will differ):

```
Checking your Fermium installation...

  ✓ Fermium 2.0.0 (/Users/you/bin/fermium)
  ✓ LLVM 18.1.8 is built in (the compiler back end)
  ✓ platform: macOS arm64 (Apple silicon)
  ✓ nothing else is needed: no Python, C compiler or LLVM to install
  - Python is optional, only for programs that say  use python : none found (that's fine)
  ✓ built in too: the REPL (fermium), the language server (fermium lsp) and the Jupyter kernel (fermium jupyter install)
  ✓ fermium build makes standalone executables with the built-in linker (lld): no C compiler needed
  ✓ compiled and ran a test program: g = 9.70 m/s²

Everything looks good! Try:  fermium   (then type  print 2 m + 30 cm )
```

What the lines mean:
- **LLVM** is the part that turns your program into fast machine code. It's *inside* the `fermium` file, so there is nothing to install.
- **Python** is only needed if you want your Fermium programs to call Python libraries (a later, optional topic). If you have Python, the line says where it is; if not, the line starts with `-` and everything in the bootcamp still works.
- The last line is the real test: Fermium worked out *g* from a pendulum's length and period.

A `✗` means something is wrong. The line below it, starting with `fix:`, tells you what to do. Do that, then run `fermium doctor` again.

## Step 5: Try the interactive prompt

Type `fermium` on its own and press Return:

```
fermium
```

You're now *inside* Fermium. The prompt changes to `fm>`. Type a line and press Return; Fermium answers straight away:

```
fm> print 2 m + 30 cm
2.30 m
fm> print 1 mi in km
1.61 km
```

It added 2 metres and 30 centimetres correctly! (Answers show 3 significant figures unless you ask for more: `print 1 mi in km to 6 digits` gives `1.60934 km`.) This interactive prompt is called the **REPL** (Read–Evaluate–Print Loop: it reads a line, works it out, prints the answer, and repeats). It's perfect for quick calculations.

To leave the REPL and go back to the normal Terminal, type `:quit` (or press **Control + D**).

## Step 6: Your first program in a file

For anything longer than a line or two, you write a **program**: a text file with instructions, which Fermium runs from top to bottom. Fermium programs end in `.fm`.

### Make a folder for your programs

Keep your programs together in one folder. This makes a folder called `physics` in your home folder and goes into it (`cd` means "change directory"; a *directory* is the same thing as a folder):

```
mkdir -p ~/physics
cd ~/physics
```

### Get a code editor

You need a **text editor** made for code. We recommend **Visual Studio Code** ("VS Code"), which is free:

1. Download it from **https://code.visualstudio.com** and install it (on a Mac, drag it into your Applications folder).
2. Open VS Code. Choose **File → Open Folder…** and open your `physics` folder.
3. *(Optional but nice)* The Fermium extension for VS Code colours your code and lets you type `\theta` then Tab to get `θ`. It lives in the folder `editors/vscode/` of the Fermium source code; `editors/vscode/README.md` there explains how to install it.

> **Don't use TextEdit** (the Mac's built-in editor) for code: it likes to turn `"` into curly quotes `“ ”` and save files as "rich text", which Fermium can't read. If you must use it, choose **Format → Make Plain Text** first.

### Write the program

1. In VS Code choose **File → New File…**, and save it (⌘ S on a Mac, Ctrl + S on Linux) inside your `physics` folder as `hello.fm`.
2. Type this into it and save again:

```fermium
# My first Fermium program
print "Hello, physics!"
print 9.81 m/s^2 * 3 s
```

3. Go back to the Terminal (make sure you're in the `physics` folder: `cd ~/physics`) and run it:

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

If a program seems stuck (nothing happens and the prompt doesn't come back), press **Control + C**. Fermium stops the program and prints `stopped by Ctrl+C`. (In a Jupyter notebook that doesn't work yet: the interrupt button doesn't stop a running cell, so use **Kernel → Restart Kernel** instead.)

The most common setup problems:

| What you see | What it means | What to do |
|---|---|---|
| `command not found: fermium` | The Terminal can't find Fermium in your PATH | Close the Terminal window and open a new one. Check the file is there with `ls ~/bin` (it should list `fermium`). Check Step 3e: `echo $PATH` should show your home folder followed by `/bin`. |
| `No such file or directory` in Step 3b | The download has a different name or is somewhere else | `ls ~/Downloads` shows what's there. Browsers sometimes add ` (1)` to a name when you download twice. |
| `permission denied: fermium` | The file isn't marked as a program | Do Step 3c: `chmod +x ~/bin/fermium`. |
| *"fermium" cannot be opened because the developer cannot be verified*, or *Apple could not verify…* (Mac) | Gatekeeper's quarantine | Do Step 3d: `xattr -d com.apple.quarantine ~/bin/fermium`. |
| `bad CPU type in executable` (Mac) or `cannot execute binary file: Exec format error` (Linux) | You downloaded the file for a different kind of computer | Check the table at the top of this lesson and download the other file. On an Intel Mac, build it from the source. |
| `can't find the file 'hello.fm'` | The Terminal is in a different folder from your file (or the name is spelled differently) | `cd` into the folder where you saved the file, and `ls` to check the file is there. |

## Building Fermium from the source

If there is no download for your computer yet (an Intel Mac, for example), you can build `fermium` yourself. It takes longer (the first build can take half an hour) and needs a few tools that programmers use:

1. Get the Fermium source folder (GitHub's green **Code → Download ZIP** button, then unzip it and rename the folder to `fermium`; or `git clone`), and put it in your home folder.
2. Install **Rust** from **https://rustup.rs** (it installs `cargo`, Rust's build tool, into `~/.cargo/bin`), and the **LLVM 18** development files: `rust/BUILD.md` in the source folder lists the one command for a Mac (Homebrew) and for Ubuntu.
3. Build and install it:

```
cd ~/fermium
make install
```

This puts `fermium` in `~/.cargo/bin`, which Rust's installer added to your PATH. Open a new Terminal window and continue with Step 3f above (`fermium --version`) and Step 4.

## The old Python version (Fermium 1.5, deprecated)

Before version 2.0, Fermium was written in Python. That version, **Fermium 1.5**, is kept in the source folder (`legacy/`) for one more release, because it is what the new one is checked against, but you don't need it: every example in this bootcamp is checked with the downloaded `fermium`. If you install it anyway (`python3 -m pip install -e ".[full]"` in the source folder), its command is called `fermium-legacy`, so it never gets in the way of `fermium`. `fermium --version` says which version you're running (version 2 ends in `(Rust)`).

## Exercises

1. Open the REPL (`fermium`) and work out how many seconds there are in a day (`print 1 day in s`). Leave with `:quit`.
2. Make a program `me.fm` that prints your name and your height in metres, e.g. `print 1.75 m`. Run it.
3. In the REPL, what does `print 1.75 m in ft` show? (You'll learn how this works in Lesson 1.)
4. Run `fermium --help` and read the list of commands. Which command checks a program's units without running it?

Solutions: [solutions/lesson00.md](solutions/lesson00.md)

**Next:** [Lesson 1 — Numbers & units](lesson01_numbers_units.md)
