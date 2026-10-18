# Share and Get Updates

Share your project with co-authors and bring their work into yours, without a terminal. Both actions live in the **Versions** view (Source control, then **Versions**), in the group above the timeline.

You can also reach the same controls without opening the Versions view: the **Share** button at the right end of the tab strip (`Ctrl+Alt+L`, `Cmd+Option+L`) opens a popover with a **Share versions** section. It shows the same status line, **Share** and **Get updates** buttons, and when updates from your co-authors are waiting the Share button itself turns lime.

Sharing sends your *saved versions*. Save a version first for anything you want others to see; the group reminds you when files have unsaved changes.

## Connect once

1. Open the Versions view.
2. Paste the address of the shared project into **Share this project with others**. It comes from your host (GitHub, GitLab, or your own server) and looks like `https://github.com/you/paper.git` or `git@github.com:you/paper.git`.
3. Press **Connect**.

You can change the address later with **Change address**.

Manang never asks for or stores a password. Sign-in uses what your computer already has set up for git: a credential helper or an ssh key loaded in your ssh agent. If the host refuses you, Manang says so and suggests checking your access.

## The status line

| It says | It means |
| --- | --- |
| Up to date | You and the shared copy match. |
| 2 changes to share | You have saved versions others do not have. **Share** is highlighted. |
| Not shared yet | Nothing from this project is on the shared copy. |
| Updates available | Others shared versions you do not have. **Get updates** is highlighted. |
| Updates available, and 2 changes to share | Both sides changed; use **Get updates**, then **Share**. |

The line is based on what Manang last learned. **Get updates** checks the shared copy; nothing is contacted until you press a button.

## Share

Press **Share**. Manang sends your saved versions and tells you how many. It never overwrites anyone else's work. If someone shared first, it stops and asks you to **Get updates** first.

## Get updates

Press **Get updates**.

- If you have unsaved changes, Manang first keeps them as a version called *Before getting updates*, so nothing can be lost.
- If only the shared copy moved, your project simply catches up.
- If both of you changed different things, Manang combines them into one new version.
- If you both changed **the same lines of a file**, Manang stops before touching any of your files and lists the files involved. Nothing on your computer changes. Agree with your co-author who shares first, or copy your edits aside, then try again.

Open tabs reload when updates arrive.

## When something goes wrong

- **The shared location could not be reached**: check the address and your internet connection.
- **Did not accept your sign-in**: check that you have access to the project, and that git has your credentials or ssh key.
- **Someone else has shared changes**: press **Get updates**, then **Share** again.
- **Nothing to share yet**: save a version first.

## Notes

- Everything is stored as ordinary files and standard git, so any git tool works with the same project.
- Large or binary files are shared like any others; very large files are better kept out of the project.
- This is manual and turn-based. Live co-editing is a separate feature.
