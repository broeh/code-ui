# Third-party notices

CODE_UI's original source uses the MIT license in `LICENSE`. Third-party software keeps its own licenses; the application's license does not relicense provider CLIs or trademarks.

The agent SVGs under `src/assets/agents` are from LobeHub and retain the MIT notice in that directory. Inter is supplied by `@fontsource-variable/inter` under OFL-1.1. React, Tauri, CodeMirror, Zustand, Lucide, Markdown/rendering libraries and other dependencies retain their respective licenses. Some dependencies use MPL-2.0 or additional permissive license terms.

The packaging workflow generates `THIRD_PARTY_LICENSES.txt` from installed npm packages and the locked Linux Rust dependency graph, preserving license and notice files. Supplemental texts for packages that omit license files are recorded with upstream source URLs in `tools/release/licenses`. Generation fails when a dependency lacks its license text. The resulting document is bundled with the desktop artifacts. Dependency source remains available through the versioned npm and crates.io links in that document; this also identifies the source of MPL-2.0 components, which CODE_UI uses without modification.

Provider CLIs are installed separately by users and are not included in the application artifacts. Provider names and icons identify integrations and do not imply a partnership or endorsement.
