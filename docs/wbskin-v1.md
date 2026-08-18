# WorkBuddy Skin Package v1

`.wbskin` is a ZIP archive with a data-only theme. It must not contain scripts, CSS, remote URLs, encrypted files, or symbolic links.

The Manager can export an installed theme back to this format. Exported packages contain only the two JSON files and the background/preview images referenced by them.

```text
example.wbskin
├── manifest.json
├── theme.json
├── preview.png
└── assets/
    └── background.webp
```

## manifest.json

```json
{
  "schemaVersion": 1,
  "id": "example-theme",
  "name": "Example Theme",
  "version": "1.0.0",
  "author": "Publisher name",
  "description": "Short description",
  "preview": "preview.png",
  "compatibility": {
    "manager": ">=0.1.0",
    "workbuddy": ["5.2.x", "5.3.x"]
  }
}
```

The ID must contain only lowercase ASCII letters, numbers, and hyphens. It must start and end with a letter or number and be at most 80 characters.

The Manager compatibility range supports exact semantic versions and whitespace-separated comparisons, such as `1.0.0` or `>=1.0.0 <2.0.0`.

## theme.json

```json
{
  "palette": {
    "background": "#fff8fb",
    "panel": "#fdebf1",
    "panelAlt": "#ffffff",
    "text": "#4a2934",
    "muted": "#75505d",
    "accent": "#d95f8d",
    "accentText": "#ffffff",
    "border": "#edb8cb",
    "hover": "#f8dbe6",
    "active": "#f3c7d7",
    "subtle": "#fff2f7"
  },
  "background": {
    "image": "assets/background.webp",
    "position": "right center",
    "size": "cover"
  }
}
```

All palette values must be six-digit hex colors. Background images must be local PNG, JPEG, or WebP files under `assets/`. Background `size` must be `cover` or `contain`.

## Compatibility policy

- Manager 1.x reads and exports `schemaVersion: 1`. A package with a newer schema version is rejected with an explicit compatibility error instead of being interpreted as v1.
- New optional JSON fields may be added within v1. Older Manager versions ignore fields they do not understand; publishers must not rely on an optional field unless the declared `compatibility.manager` range requires a version that supports it.
- Existing required v1 fields and their meaning will not change within Manager 1.x. A breaking format change requires a new schema version.
- The `compatibility.manager` range controls package-format and feature compatibility. `compatibility.workbuddy` separately controls which WorkBuddy DOM versions the theme supports.
- Exporting a theme from Manager 1.x always produces a validated v1 package; it does not preserve unsupported files or executable content from the source archive.

## Limits

- Maximum 16 files
- Maximum 32 ZIP entries including directories
- Maximum 20 MB archive size
- Maximum 20 MB total uncompressed size
- Maximum 256 KB for each JSON file
- Maximum 8 MB for each image file
- Maximum image width or height: 8192 pixels
- Maximum image area: 40 million pixels
- Image signatures must match their PNG, JPEG, or WebP file extensions
