<div align="center">
  <h1><img width="700" alt="image" src="https://github.com/user-attachments/assets/16859424-f9f0-4b0d-8c8a-0f77c98331ec" /></h1>
  <img alt="GitHub License" src="https://img.shields.io/github/license/Inparsian/gray-meadows-shell">
  <img alt="GitHub top language" src="https://img.shields.io/github/languages/top/Inparsian/gray-meadows-shell">
  <img alt="GitHub contributors" src="https://img.shields.io/github/contributors/Inparsian/gray-meadows-shell"/>
  <img alt="GitHub commit activity" src="https://img.shields.io/github/commit-activity/w/Inparsian/gray-meadows-shell"/>
  <img alt="GitHub Repo stars" src="https://img.shields.io/github/stars/Inparsian/gray-meadows-shell?style=flat"/>
  <img alt="GitHub forks" src="https://img.shields.io/github/forks/Inparsian/gray-meadows-shell?style=flat"/>
  <img alt="GitHub issues" src="https://img.shields.io/github/issues/Inparsian/gray-meadows-shell"/>
  <img alt="GitHub pull requests" src="https://img.shields.io/github/issues-pr/Inparsian/gray-meadows-shell"/>
</div>
<div align="center">
  <h3>
    [<a href="https://gray-meadows.inpr.sn/">wiki</a>]​&nbsp;
    [<a href="https://gray-meadows.inpr.sn/showcase">showcase</a>]​&nbsp;
    [<a href="https://trello.com/b/bzhLDyI8/gray-meadows-shell-roadmap">trello</a>]
  </h3>
</div>
<br>

My personal stand-alone Hyprland desktop shell using gtk4 and gtk4-layer-shell, re-written from scratch in Rust and C++ (originally in TypeScript + JSX using AGSv2).

⚠️ This is beta software. It can be daily-driven, but there may be bugs, performance issues, and things that otherwise need polishing. If you find any, check the Trello and issues tab before opening an issue/pull request.

## 📷 Show me the screenshots
ok

<table align="center">
  <tr>
    <td colspan="3">
      <img width="1920" height="1080" alt="1" src="https://github.com/user-attachments/assets/7fbfaec8-6ebf-4b88-bac8-9528c500d6db" />
    </td>
    <td colspan="3">
      <img width="1920" height="1080" alt="2" src="https://github.com/user-attachments/assets/47ace8c5-9aed-4f59-8f96-e77acb3c19c9" />
    </td>
    <td colspan="3">
      <img width="1920" height="1080" alt="3" src="https://github.com/user-attachments/assets/a9558e80-a0fa-49d0-b33a-bf6d560f0da8" />
    </td>
  </tr>
  <tr>
    <td colspan="3">
      <img width="1920" height="1080" alt="4" src="https://github.com/user-attachments/assets/31bfda9c-6dac-4147-b1c8-c212be6e2006" />
    </td>
    <td colspan="3">
      <img width="1920" height="1080" alt="5" src="https://github.com/user-attachments/assets/9e08dcb5-4bac-44f1-a379-c002f4755e83" />
    </td>
    <td colspan="3">
      <img width="1920" height="1080" alt="6" src="https://github.com/user-attachments/assets/6461d394-081d-4a61-9fc1-1e55074469af" />
    </td>
  </tr>
</table>

## ❤️ Sponsors
I express my deep gratitude to the following sponsors for their generous support, it means a lot to me!
- [Alexis](https://github.com/garado)

## 💡 Features

- Bar with workspace management, music controls, system tray, system stats, volume, and more
- OSD for volume
- Apps of their own, that look like the shell and follow the same `GMS_*` environment variables:
  - `gms-launcher`: a generic launcher. It shows the entries it is given on stdin and prints the one that is picked, so what it launches is up to a script. [Examples](../apps/launcher/README.md) for an application picker and an emoji picker are included
  - `gms-session`: the logout selector (lock, log out, suspend, hibernate, reboot, shut down)
- Right sidebar:
  - Notifications
  - Audio controls
  - Brightness of all monitors, internal and external
  - Calendar
  - Weather
  - Timers *(soon)*

## 🤔 Why the name "Gray Meadows"?
grayscale color scheme, first screenshot of the agsv2 version used a foggy forest wallpaper. was too lazy to come up with a better name

## 🤔 Niri support?
Not any time soon.

## 🔵 Recommended icon theme
Since Gray Meadows is intended to have a monochrome/grayscale aesthetic, I recommend you use an icon theme that matches it. My personal favorite is [Besgnulinux Monochrome](https://www.gnome-look.org/p/2151189/), but any monochrome or grayscale icon theme should work well.
