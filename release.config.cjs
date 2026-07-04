module.exports = {
  branches: [
    { name: 'main' },
    { name: 'develop', channel: 'next', prerelease: 'next' },
  ],
  tagFormat: 'v${version}',
  plugins: [
    [
      '@semantic-release/commit-analyzer',
      {
        preset: 'conventionalcommits',
      },
    ],
    [
      '@semantic-release/release-notes-generator',
      {
        preset: 'conventionalcommits',
      },
    ],
    '@semantic-release/changelog',
    [
      '@semantic-release/exec',
      {
        prepareCmd:
          'node scripts/sync-release-version.js ${nextRelease.version} && node scripts/build-release-assets.js ${nextRelease.version}',
      },
    ],
    '@semantic-release/npm',
    [
      '@semantic-release/git',
      {
        assets: ['package.json', 'package-lock.json', 'Cargo.toml', 'Cargo.lock', 'CHANGELOG.md'],
        message:
          'chore(release): ${nextRelease.version} [skip ci]\n\n${nextRelease.notes}',
      },
    ],
    [
      '@semantic-release/github',
      {
        assets: [
          {
            path: 'dist/release/*.tar.gz',
            label: 'Release archive',
          },
        ],
        failCommentCondition: false,
      },
    ],
  ],
};
