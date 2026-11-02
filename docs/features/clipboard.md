# Clipboard history

Status: in, history only. Navo records what you copy and shows it in a palette. The built-in checks pass. A pass on a real Windows desktop is still to come.

## What it is

Navo keeps a list of what you copy on this PC. It records text, such as links and commands, and file lists. A palette shows the list, newest first. You search it, pick an entry and paste it back.

The clipboard history is a module. You can turn it off in Settings.

## What you can do

- Press Ctrl+Shift+V in the Navo window to open the palette.
- Press Win+Alt+V from any app to open the palette over that app. Enter pastes the entry back into the app you were in. This works only while Navo runs.
- Type to search. Matches are highlighted in the list and in the preview.
- Press Tab and Shift+Tab to switch between the tabs above the list.
- Press Enter to use the entry. In the Navo window it copies the entry. From Win+Alt+V it pastes into the previous app. If Navo itself was in front, it only copies.
- Press Ctrl+C to copy the entry without pasting.
- Press Delete to remove one entry.
- Press Ctrl+K in the palette for the actions of the selected entry: Copy, Go to, Copy path, Paste into the active pane and Delete. Go to, Copy path and the paste action apply to file lists.
- Press Esc to clear the search, then to close the palette.

A copied entry moves to the top when you copy the same text or the same files again. The palette does not list the same thing twice.

The status line at the bottom of the palette confirms an action, for example "Copied" or "Deleted". The first time you open the palette, one line says: "Navo keeps what you copy on this PC. Settings › Clipboard turns it off." It shows once per profile.

## Commands in Ctrl+K

- Open clipboard history.
- Pause or resume clipboard history.
- Clear clipboard history.
- Delete all clipboard data.

These commands are hidden when the module is off or not running.

## Privacy

- History stays on this PC. It is stored in the profile data folder, in its own file, `clipboard.db`. Nothing is sent anywhere.
- History text is stored as plain text. Anyone who can read your profile folder can read it.
- Copies that password managers or Windows mark as private are never recorded.
- Images and other formats are never recorded. Empty and whitespace-only text is not recorded.
- File lists keep the paths only, never the file contents.
- Navo never writes the text you copy to its logs, crash records or diagnostic reports.
- If the history file is damaged, Navo sets it aside as `clipboard.db.corrupt-` and a number. It starts with an empty history. The palette shows one line about it.

## Pause, clear and delete

- Pause stops recording. Run the same command again to resume. The palette shows a notice while recording is paused. The entries you already have stay.
- Clear history removes every entry in the history. It asks first and shows how many entries go.
- Delete all removes the whole clipboard database and any set-aside `clipboard.db.corrupt-` copies. It asks first and shows the counts. It cannot be undone. It works while the module is off.
- If a clear or a delete is refused, Navo says "not deleted" and keeps the data.

## Turn it off

Open Settings with Ctrl+, and choose Clipboard. Turn off "Clipboard and saved items". Navo then records nothing and does not register Win+Alt+V. The commands leave Ctrl+K. Your data stays on disk until you use Delete all.

## Settings

Settings › Clipboard has three rows. They are stored in `navo.toml` under `[clipboard]`.

| Setting | Default | What it does |
| --- | --- | --- |
| Clipboard and saved items (`enabled`) | on | Turns the module on or off |
| Record clipboard history (`record-history`) | on | Records what you copy; off pauses recording |
| Open from anywhere (`summon-key`) | Win+Alt+V | Opens the palette over any app; leave empty for none |

If another app already uses the key, the settings row says so.

## Limits

- The history keeps the newest 500 entries. Older ones go.
- One text entry can be at most 256 KiB. A bigger copy is not recorded.
- One file list can hold at most 10,000 paths. A bigger list is not recorded.
- All history text together is capped at 32 MiB.

## Done and coming

- Done: recording of text, links and file lists, the palette in the window and from anywhere, paste back, search, pause, clear, delete all, and the Settings rows.
- Windows only: the Record row and the Win+Alt+V key.
- Coming: more clipboard features in a later release.

## Open questions

- A pass on a real Windows desktop: paste back into other apps and the global key.
