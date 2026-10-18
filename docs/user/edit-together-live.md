# Edit together live

Live editing lets two or more people type in the same text file at the same time and see each other's edits and carets as they happen. It works on one text file per session (Typst, LaTeX, Markdown or any other text file). Figures and PDFs are not shared yet.

Your file on disk stays an ordinary text file. The shared state lives only in memory while the session runs. When you press `Ctrl+S` (`Cmd+S`), Manang saves exactly what you see; nothing about the session is written into your project.

## What you need

- **A relay address**, once, to start a session. A relay is a small server where the people in a session meet. It passes on encrypted data and cannot read your text (see [What the relay can see](#what-the-relay-can-see)). Whoever runs the relay for your group gives you its address, for example `wss://relay.example.org`.
- **An invite**, to join a session. The person who shares the file sends it to you. It already contains the relay address, so you do not need to set one to join.

## Set the relay (to share)

1. Open **Settings** (`Ctrl+,`) and find the **Live** section, or choose **Share this file live** once: if no relay is set, a bar above the editor says so and has an **Open settings** button.
2. Enter the address under **Relay address**. Use `wss://` for a relay behind TLS, or `ws://` for one you run on your own network.
3. Optionally set **Name shown to collaborators**. If you leave it empty, your system user name is used.

## Share a file

1. Open the text file you want to work on together.
2. Open the **File** menu and choose **Share this file live…** (or run it from the command palette, `Ctrl+K`).
3. A bar appears above the editor. It says **Connecting**, then **Live**. The invite has been copied to your clipboard. If the clipboard is not available, the invite appears in a field in the bar, already selected.
4. Send the invite to the people you want to work with. Use **Copy invite** in the bar to copy it again.

An invite is a long link that starts with `manang-live://`. **Anyone who has it can edit the file with you**, so send it only to the people you mean, through a channel you trust.

## Join a session

1. Open the **File** menu and choose **Join a live session…**.
2. Paste the invite and press `Enter`.
3. Manang opens the file the invite names if your project has it; otherwise it uses the file in front of you.

If your copy of the file is **identical** to the host's at the moment they started sharing, you join straight away.

If it is **different**, Manang does not replace anything silently. It asks first, and explains that joining replaces your copy with the host's version. If you agree, your copy is saved first to a folder outside your project (`live-trash` in Manang's settings folder), then the host's text arrives. If your copy is empty, there is nothing to lose and you join directly.

## In the session

- The bar shows the state of the connection and who is in the session. Each collaborator has a name and a mark next to it; the same mark (a colour mix and a line style) is drawn at their caret in the text, with their name above it. Selections are tinted in the same colour.
- Everyone's edits merge. If two people type at the same spot at the same moment, both edits are kept.
- **Undo is yours.** `Ctrl+Z` takes back your own edits only, never someone else's.
- Only one file can be live at a time. Closing the file's tab leaves the session.

### States

| The bar says | What it means |
| :--- | :--- |
| **Connecting** | Opening the connection and exchanging the document with the others. |
| **Live** | Connected. Edits flow both ways. |
| **Reconnecting** | The connection dropped. Keep typing: your edits are kept and merge automatically when the connection is back. Manang retries on its own, waiting a little longer each time. |
| **Session stopped** | Something went wrong that Manang cannot repair on its own; the bar says what. Leave and, if needed, join again with the invite. |

## Leave

Choose **Leave** in the bar (or **Leave live session** in the File menu). Edits still on their way get a moment to arrive, then the session ends. The file stays open as it was; save it as usual. Leaving does not delete anything for the others, who carry on.

## Saving, versions and Git

- Each person saves their own copy. After a session everyone's file contains the same merged text, but each person decides when to save it.
- Version history and Git work as before. Do not use **Get updates** on the live file while you are in a session: Manang does not block it yet, and changing the file from outside while a session runs is not supported. Leave first.

## What the relay can see

The invite contains a secret key. The key is part of the link after the `#`, so it is never sent to the relay. Everything that goes through the relay is encrypted with it (XChaCha20-Poly1305) before it leaves your computer.

- The relay can see: that a connection to a room exists, how large and how frequent the messages are, and when people connect and leave. A room is named by a number derived from the key; the relay cannot turn it back into the key.
- The relay cannot see: your text, your edits, your name, your caret, the file name, or the key.
- The relay stores nothing and forgets a room when the last person leaves.

What this does not give you:

- **No accounts.** Everyone holding the invite is trusted equally. A name in the bar is a claim, not a login: anyone with the invite could use another person's name.
- **No forward secrecy.** Someone who obtains the invite later and recorded the encrypted traffic earlier could read it. To rotate the key, leave and start a new session.
- If you do not trust a relay operator with the *metadata* above, run your own: the relay is a small program (`manang-relay`) with no dependencies on a cloud service.

## Troubleshooting

- **"Set a relay in Settings to share live."** Sharing needs a relay address (joining does not). See [Set the relay](#set-the-relay-to-share).
- **"This invite is not valid."** The link was cut or changed. Ask for it again and paste it whole, including everything after the `#`.
- **Stuck on Connecting or Reconnecting.** The relay is not reachable from your network (firewall, wrong address, relay down). Check the address in Settings; the person who shared can see the same state in their bar. Your edits are kept in the meantime.
- **"The live document was re-read from the session."** A step could not be applied, so Manang replaced your view with the session's text to stay consistent. Edits you had typed in the last moments before may need to be typed again. This is rare; please report it.
- **Both people see different text after reconnecting.** Leave and join again. If it keeps happening, please report it with the relay address kind (`ws://` or `wss://`) and what you were doing.
