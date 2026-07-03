# blipcoard docs site

This Docusaurus site publishes the public blipcoard documentation.

Run from the repository root:

```bash
npm run docs:start
npm run docs:build
npm run docs:serve
```

Run from this directory:

```bash
npm start
npm run build
npm run serve
```

The source docs live under `site/docs/`. Generated `.docusaurus/`, `build/`,
and `node_modules/` directories are local-only artifacts.

## Publishing

`.github/workflows/docs.yml` builds the site on pull requests. On pushes to
`develop`, the same workflow publishes `site/build` to the `gh-pages` branch.
Do not commit `site/build/`; the workflow owns the published branch contents.
