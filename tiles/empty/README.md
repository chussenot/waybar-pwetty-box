# `empty` tile

A compact, narrow tile for a niri desktop with **no windows** — it doesn't
warrant a full-width representation. Just the shortcut number stacked over a dim
hollow "empty" ring, center-aligned.

```
┌──────┐
│  7   │   shortcut (dim)
│  ◯   │   empty ring
└──────┘
```

## Using it

```jsonc
"cffi/pwetty#desk7": {
  "module_path": ".../libpwetty_box.so",
  "tile": "empty",
  "exec": "echo '{\"shortcut\": 7, \"active\": false}'"
}
```

Width defaults to 56px. See [`schema.json`](./schema.json) for the data contract
(`shortcut`, optional `active`). When `active` is true the tile gets the same
accent card as the `claude` tile, so the focused empty desktop still stands out.
