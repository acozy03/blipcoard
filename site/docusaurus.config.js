// @ts-check
import {themes as prismThemes} from 'prism-react-renderer';
import npm2yarn from '@docusaurus/remark-plugin-npm2yarn';

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'blipcoard',
  tagline: 'Runtime-first clipboard routing for agent workflows',
  favicon: 'img/blipcoard-removebg-preview.png',

  future: {
    v4: true,
  },

  url: 'https://blipcoard.github.io',
  baseUrl: '/blipcoard/',
  organizationName: 'blipcoard',
  projectName: 'blipcoard',

  onBrokenLinks: 'throw',
  markdown: {
    hooks: {
      onBrokenMarkdownLinks: 'throw',
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          routeBasePath: 'docs',
          sidebarPath: './sidebars.js',
          editUrl: 'https://github.com/blipcoard/blipcoard/tree/develop/site/',
          remarkPlugins: [[npm2yarn, {sync: true, converters: ['yarn', 'pnpm']}]],
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      }),
    ],
  ],

  plugins: [
    [
      '@easyops-cn/docusaurus-search-local',
      {
        hashed: true,
        indexDocs: true,
        indexPages: true,
        indexBlog: false,
        language: ['en'],
        docsRouteBasePath: '/docs',
      },
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      image: 'img/blipcoard-removebg-preview.png',
      colorMode: {
        respectPrefersColorScheme: true,
      },
      navbar: {
        logo: {
          alt: 'blipcoard logo',
          src: 'img/blipcoard-logo.png',
        },
        items: [
          {
            type: 'html',
            position: 'right',
            value:
              '<a class="navbar__item navbar__link blip-github-link" href="https://github.com/blipcoard/blipcoard" target="_blank" rel="noopener noreferrer" aria-label="GitHub repository"></a>',
          },
        ],
      },
      prism: {
        theme: prismThemes.github,
        darkTheme: prismThemes.vsDark,
        additionalLanguages: ['bash', 'diff', 'powershell', 'rust', 'toml'],
        magicComments: [
          {
            className: 'theme-code-block-highlighted-line',
            line: 'highlight-next-line',
            block: {start: 'highlight-start', end: 'highlight-end'},
          },
          {
            className: 'code-block-error-line',
            line: 'error-next-line',
            block: {start: 'error-start', end: 'error-end'},
          },
          {
            className: 'code-block-success-line',
            line: 'success-next-line',
            block: {start: 'success-start', end: 'success-end'},
          },
        ],
      },
    }),
};

export default config;
