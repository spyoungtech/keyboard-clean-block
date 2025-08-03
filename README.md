# Keyboard Clean Block

A simple MacOS app to block keyboard input so you can clean your keyboard without 
unwanted keyboard inputs reaching the system.


## Installation

The easiest way to install is to download the dmg from the [Releases page](https://github.com/spyoungtech/keyboard-clean-block/releases) 
which will contain a signed/notarized app ready to install just like any other MacOS dmg.

The first time you launch and use the app, you'll be asked to grant it permissions for input monitoring and 
accessibility features. These are used to intercept keyboard events.


## About

Although there are a couple existing applications that provide this functionality, none of them were Open Source and I 
didn't want to give elevated permissions to those apps if I could not verify the source code.

## Known limitations

- Media keys (e.g., volume keys and play/pause buttons) and the power button (lock) will still function even while the keyboard block is active.
- If you press the power button and your system goes to the lock screen and input is not blocked while on the lock screen
