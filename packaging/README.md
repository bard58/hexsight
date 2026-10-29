# Packaging drafts

These files are drafts. They are not submitted anywhere. Do not submit them before the first signed or hashed GitHub release exists.

Placeholders to replace in every file:

| Placeholder | Meaning |
|---|---|
| `0.1.0` | The version (also in the URLs) |
| `REPLACE-WITH-SHA256` | The SHA-256 hash of the release file (lower case for Scoop and Chocolatey, upper case is also valid for winget). winget and Chocolatey use the exe. Scoop uses the portable zip. Read it from the `.sha256` file of that file in the release |

Each channel has its own rules. Check them again before you submit.

| Channel | Folder | Notes |
|---|---|---|
| winget | `winget/` | Pull request to `microsoft/winget-pkgs`. One version for each pull request. The installer type is `portable`, so the exe needs no installer. Run `winget validate` on the folder first. |
| Scoop | `scoop/` | The main bucket does not fit (it needs a well-known command-line tool). Try a third-party bucket or the Extras bucket and ask in their contribution guide first. The manifest uses the portable zip (`Hexsight.exe` and the marker file `Hexsight.portable`). Hexsight then keeps its data in `Hexsight-data` next to the exe, and `persist` keeps that folder across updates. |
| Chocolatey | `chocolatey/` | Needs an account on community.chocolatey.org and a human review. The package downloads the exe from the GitHub release. It does not embed the binary. |

Sources (checked 2026-10-24): [winget manifest docs](https://learn.microsoft.com/en-us/windows/package-manager/package/manifest), [winget-pkgs policies](https://github.com/microsoft/winget-pkgs/blob/master/doc/Policies.md), [Scoop main bucket criteria](https://raw.githubusercontent.com/wiki/ScoopInstaller/Scoop/Criteria-for-including-apps-in-the-main-bucket.md), [Chocolatey moderation](https://docs.chocolatey.org/en-us/community-repository/moderation/).
