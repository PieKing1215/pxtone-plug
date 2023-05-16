# pxtone Plug
A **work in progress** VST®3 and CLAP instrument plugin for pxtone woices.

<img src="https://github.com/PieKing1215/pxtone-plug/assets/13819558/469d9319-dfaa-4da1-bdcc-99c186ab4149" width=150 />

### Current Features
- ptVoice support (temporarily hard coded file)
- Note On/Off events
- Velocity

### Planned Features
- ptNoise support
- Support for more MIDI events

Feel free to suggest more

## Building

1. Install [Rust](https://www.rust-lang.org/learn/get-started)
2. Clone this repo
3. Run `cargo xtask bundle pxtone-plug`

If it succeeds, there should be a .clap and a .vst3 somewhere in the `target/bundled/` folder.<br>
If not, or if it fails, please [open an issue](https://github.com/PieKing1215/pxtone-plug/issues).

## Licensing & Attribution

pxtone Plug's code is licensed under the [GNU GPLv3 License](COPYING) (as required by Steinberg's terms [(link)](https://steinbergmedia.github.io/vst3_dev_portal/pages/FAQ/Licensing.html#q-i-would-like-to-share-the-source-code-of-my-vst-3-plug-inhost-on-github-or-other-such-platform))

This repo includes code ported from pxtone (and uses [rust-pxtone-sys](https://github.com/PieKing1215/rust-pxtone-sys) which contains pxtone source code):<br>
[pxtone](https://pxtone.org/developer/) © [STUDIO PIXEL](https://studiopixel.jp)

Parts of this program are based off of example code from [NIH-plug](https://github.com/robbert-vdh/nih-plug/tree/master), which is licensed under the [ISC license](LICENSE_NIH-PLUG_ISC).

VST® is a trademark of Steinberg Media Technologies GmbH, registered in Europe and other countries.
