<h1>pxtone Plug<br>
  <a href="https://github.com/PieKing1215/pxtone-plug/actions/workflows/autobuild.yml"><img src="https://github.com/PieKing1215/pxtone-plug/actions/workflows/autobuild.yml/badge.svg" /></a>
</h1>

A **work in progress** VST®3 and CLAP instrument plugin for playing pxtone woices.

<img src="https://github.com/PieKing1215/pxtone-plug/assets/13819558/469d9319-dfaa-4da1-bdcc-99c186ab4149" width=150 />

### Current Features
- Load ptVoice from file
- Loaded woice persists across host restarts
- Note On/Off events + velocity

### Planned Features
- ptNoise support
- Support for more MIDI events
- Pretty GUI
- Mac support ([issue](https://github.com/PieKing1215/pxtone-plug/issues/1))

[Feel free to suggest more!](https://github.com/PieKing1215/pxtone-plug/issues/new?assignees=&labels=enhancement&projects=&template=feature_request.md&title=)

## Installing

There are no stable releases right now.<br>
Automated dev builds can be downloaded in [here](https://github.com/PieKing1215/pxtone-plug/actions/workflows/autobuild.yml?query=branch%3Amaster) after signing in to GitHub.

## Usage

Use the `.vst3` file as a VST3 plugin or use the `.clap` file as a CLAP plugin.<br>
If you need to run pxtone Plug as a VST2, you must use something like [vst3shell](https://www.kvraudio.com/forum/viewtopic.php?t=565924) or [Element](https://github.com/kushview/element) to bridge the gap.

## Building

1. Install [Rust](https://www.rust-lang.org/learn/get-started)
2. On Linux, you may need to install some extra system libraries.<br>
   GitHub Actions needed `sudo apt install -y freeglut3-dev libxcursor-dev libgtk-3-dev libxcb-icccm4-dev libx11-xcb-dev libxcb-dri2-0-dev`
4. Clone this repo
5. Run `cargo xtask bundle pxtone-plug --release`

If it succeeds, there should be a .clap and a .vst3 somewhere in the `target/bundled/` folder.<br>
If not, or if it fails, please [open an issue](https://github.com/PieKing1215/pxtone-plug/issues/new?assignees=&labels=bug&projects=&template=bug_report.md&title=).

## Licensing & Attribution

pxtone Plug's code is licensed under the [GNU GPLv3 License](COPYING) (as required by Steinberg's terms [(link)](https://steinbergmedia.github.io/vst3_dev_portal/pages/FAQ/Licensing.html#q-i-would-like-to-share-the-source-code-of-my-vst-3-plug-inhost-on-github-or-other-such-platform))

This repo includes code ported from pxtone (and uses [rust-pxtone-sys](https://github.com/PieKing1215/rust-pxtone-sys) which contains pxtone source code):<br>
[pxtone](https://pxtone.org/developer/) © [STUDIO PIXEL](https://studiopixel.jp)

Parts of this program are based off of example code from [NIH-plug](https://github.com/robbert-vdh/nih-plug/tree/master), which is licensed under the [ISC license](NIH-PLUG_LICENSE).

VST® is a trademark of Steinberg Media Technologies GmbH, registered in Europe and other countries.
