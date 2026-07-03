# Docs Maintenance

The public documentation source of truth is `site/docs/`.

Repository-root Markdown files can still exist for local process, historical
context, or short pointers, but user-facing product, architecture, operations,
reference, contributor, and roadmap docs should live in the Docusaurus site.

## When To Update Docs

Update docs in the same pull request when a change affects:

- CLI commands, flags, output, or error messages
- daemon API commands, payloads, response fields, or error codes
- config keys, environment variables, path defaults, or service behavior
- database schema, migrations, backup, restore, or blob storage rules
- desktop workflows, preview behavior, or policy controls
- packaging, installation, upgrade, or uninstall behavior
- security, privacy, workspace policy, or agent access defaults
- phase status, roadmap scope, or architecture boundaries

If the behavior is user-visible, operationally important, or likely to guide a
future contributor, document it.

## Adding Or Moving Pages

When adding a page:

1. Put it under the appropriate `site/docs/` section.
2. Add it to `site/sidebars.js`.
3. Update navbar or footer links in `site/docusaurus.config.js` only when the
   page is a primary entry point.
4. Fix migrated relative links.
5. Run `npm run docs:build`.

Avoid duplicate public docs that explain the same behavior differently. Prefer
one canonical page and link to it from guides or overview pages.

## Review Ownership

The implementation owner is responsible for updating affected docs.

Additional review expectations:

- product or roadmap changes need review from the phase or product owner
- architecture, API, storage, or security changes need technical review
- CLI and operations changes need someone to check commands and paths against
  the source
- docs-only navigation or publishing changes need a docs build check
- release-impacting changes need a release-note callout in the PR

## Release Notes

Until the project has a dedicated changelog, PR descriptions should call out
release-impacting changes:

- user-facing features
- breaking CLI or daemon API changes
- migrations or downgrade limitations
- config or path changes
- packaging and service lifecycle changes
- security or privacy default changes
- known limitations

When a maintained changelog is added, these PR callouts should become the source
material for release notes.

## Publishing

Pull requests build the docs site but do not publish it.

After docs changes merge to `develop`, GitHub Actions builds `site/` and pushes
the generated static site to the `gh-pages` branch. Do not commit `site/build`
to `develop`.
