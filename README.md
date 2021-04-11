# Teehee - a modal terminal hex editor

![AUR version](https://img.shields.io/aur/version/teehee)

Inspired by Vim, Kakoune and Hiew.

## Installation
Arch Linux users: The package for Arch Linux is available on [AUR](https://aur.archlinux.org/packages/teehee/).

Others: Just run `cargo install teehee`! If you don't have rust, you can get it from [rustup.rs](https://rustup.rs).
The application will be available as the executable `teehee`. More installation options may be coming in the future.

## Motivation

Reverse engineers, software engineers and other IT people often need to work with binary files. Hex editors are usually the go-to tool for dealing with binary file formats when a more specialized tool isn't available. Many of the existing hex editors lack support for modal editing, which Vim/Kakoune/Emacs users will miss. Hiew supports it to an extent, but it's non-free software, and its keybinds are unintuitive. Teehee is designed to offer a native-feeling experience to Kakoune and Vim users, while also providing additional hex editing capabilities like coloured marks for regions of data and encryption/compression scripts.

## Demo

[![asciicast](https://asciinema.org/a/349728.svg)](https://asciinema.org/a/349728)

Teehee supports multiple selections, efficient selection modifying commands and various data editing operations.

## Design

![image](https://user-images.githubusercontent.com/6651822/87162730-010efe00-c2cf-11ea-8a0e-f90fbd209cec.png)

## Implemented keybinds
* `hjkl` for movement (press shift to extend selection instead)
```
    ^
< h j l >
    k
    v
```
* `g`[`hjkl`] for jumping (`G`[`hjkl`] to extend selection instead)
    * `h`: to line start
    * `l`: to line end
    * `k`: to file start
    * `j`: to file end
    * `<count>g` jumps to offset, `<count>G` extends to offset
* `;` to collapse selections to cursors
* `<a-;>` (alt and ;) to swap cursor and selection end
* `<a-s>` (alt and s) to split selection to multiple selections of size...
    * `b`: 1 byte
    * `w`: 2 bytes (Word)
    * `d`: 4 bytes (Dword)
    * `q`: 8 bytes (Qword)
    * `o`: 16 bytes (Oword)
    * `n`: delimited by null bytes
    * `/`: matching a text pattern (`?` for hex pattern)
* `d` to delete selected data from buffer
* `i` to enter insert mode at the beginning of selections (`I` to insert hex instead of ascii)
    * `a` instead of `i` to enter append mode instead
    * `c` instead of `i` to delete selection contents, then enter insert mode
    * `<c-n>` to insert a null byte in ascii mode
    * `<c-o>` to switch between ascii and hex inserting
* `(` and `)` to cycle main selection
* `<space>` to keep only main selection, `<a-space>` to keep all selections but main
* `r<key>` to replace a each selected character with the ASCII character given
    * `R<digit><digit>` instead of `r` to replace with a single hex character instead
    * `r<c-n>` to replace with null bytes
* `y` to yank/copy selections to register `"`
* `p` to paste register `"` contents from `y`/`d`/`c`
* `s` to collapse selections to those matching a text pattern (`S` for hex pattern)
* `M` to measure length of current main selection (in bytes)
* `:` to enter command mode
	* `:q` to quit
	* `:q!` to force quit (even if buffer dirty)
	* `:w` to flush buffer to disk
	* `:w <filename>` to save buffer to named file
	* `:wa` to flush all buffers to disk
	* `:e <filename>` to open a new buffer
	* `:db` to close a buffer
	* `:db!` to close a buffer even if dirty
	* `:wq` to flush buffer, then quit

Entering a pattern:

* `<C-w>` to insert a wildcard
* `<C-o>` to switch input mode (ascii <-> hex)
* `<esc>` to go back to normal mode
* `<enter>` to accept pattern
* arrow keys, `<backspace>` and `<delete>` also supported

Counts:
* The following commands maybe prefixed by a count:
    * Movement (`hjkl` and `HJKL`)
    * Selection modification (`()<space><a-space>`)
    * Jump to offset (`g` and `G`)
    * Paste (`p`)
    * (In split mode) `bwdqon`
* Counts are inputted by typing digits 0-9 (in hex mode, 0-f).
* `x` switches between hex and decimal mode.
* Note that `a-f` may shadow some keys, so switch out of hex mode before running
a command.
* Example: `y5p`: yank the selection and paste it 5 times.
* Example: `50l`: Move 50 bytes to the right.
* Example: `x500g`: Jump to offset 0x500
* Example: `<a-s>x12xb`: Split selection into parts of 0x12 bytes.

# Releases
Releases are signed with the following PGP key:
`9330E5D6861507BEFBF1893347E208E66179DC94`. The source code can be found on
the [GitHub releases page](https://github.com/Gskartwii/teehee/releases), along
with the signature of the source code tgz.
