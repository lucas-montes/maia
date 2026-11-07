# Waybar: the `/agenda` module

Supersedes the socket-based sketch in `context/plans/system-waybar.md` (that daemon no longer
exists). This is a copy-pasteable Waybar `custom` module against the real, currently-running
`maia` service's `GET /agenda` endpoint.

`/agenda` is Bearer-authed exactly like every other `maia` route — there is no unauthenticated
variant for Waybar to hit. It returns:

```json
{
  "overdue": [{"id": "...", "title": "Pay rent", "dueDate": 1762300800000, "dueTimeMinutes": 720}],
  "dueToday": [{"id": "...", "title": "Water the plants", "dueDate": 1762387200000, "dueTimeMinutes": null}],
  "nextMeeting": {"id": "...", "title": "Standup", "date": 1762387200000, "startTimeMinutes": 540}
}
```

Both task lists are plain arrays (empty, not an error, when nothing is due); `nextMeeting` is
`null` when no upcoming time-blocked task exists. "Next meeting" is a proxy — the next task with a
`startTimeMinutes` set — until real calendar sync exists; same shape either way.

## The one-liner

Waybar's `custom` module protocol wants one line of JSON on stdout:
`{"text": "...", "tooltip": "...", "class": "..."}`. This folds `/agenda` into that shape with
`jq`, picking the single most urgent thing: overdue count first, then "due today" count, then the
next meeting's time, else an all-clear.

```sh
curl -s -H "Authorization: Bearer $MAIA_API_KEY" http://127.0.0.1:3030/agenda | jq -c '
  def minutes_label($m): ($m / 60 | floor | tostring) + ":" + (($m % 60) | tostring | if length < 2 then "0" + . else . end);
  (.overdue | length) as $od |
  (.dueToday | length) as $dt |
  if $od > 0 then
    {text: "⚠ \($od) overdue", tooltip: ([.overdue[].title] | join("\n")), class: "overdue"}
  elif $dt > 0 then
    {text: "📋 \($dt) due today", tooltip: ([.dueToday[].title] | join("\n")), class: "due-today"}
  elif .nextMeeting then
    {text: "📅 " + .nextMeeting.title + " " + minutes_label(.nextMeeting.startTimeMinutes), tooltip: .nextMeeting.title, class: "meeting"}
  else
    {text: "✓ clear", tooltip: "Nothing due", class: "clear"}
  end
'
```

`$MAIA_API_KEY` is the same Bearer key the phone and desktop app use
(`services.maia.apiKey` / `/etc/maia/config.json`'s `api_key`); export it in whatever shell profile
sources Waybar's environment, or inline it directly in the module's `exec` if the config file isn't
shared with a login shell.

## The module (`~/.config/waybar/config`)

```jsonc
"custom/agenda": {
  "exec": "~/.config/waybar/scripts/agenda.sh",
  "return-type": "json",
  "interval": 60,
  "on-click": "xdg-open http://127.0.0.1:3030/docs"
}
```

Put the `curl | jq` one-liner above in `~/.config/waybar/scripts/agenda.sh` (executable), referenced
by `exec` rather than inlined, since Waybar's `exec` string has its own quoting rules that fight
with `jq`'s. Add `"custom/agenda"` to one of the `modules-left` / `modules-center` /
`modules-right` arrays in the same config to place it.

```css
/* ~/.config/waybar/style.css */
#custom-agenda.overdue { color: #f38ba8; font-weight: bold; }
#custom-agenda.due-today { color: #f9e2af; }
#custom-agenda.meeting { color: #89b4fa; }
#custom-agenda.clear { color: #a6e3a1; }
```

None of this (module config, script, CSS) is deployed by this repo's Nix config — the
home-manager-side Waybar setup is the user's own dotfiles. This doc is the copy-paste source of
truth; `services.maia` only guarantees `/agenda` stays authed, shaped like this, and running.
