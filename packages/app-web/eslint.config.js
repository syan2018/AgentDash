import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    rules: {
      // 允许以 `_` 为前缀的参数/变量/捕获表示"有意未用"，避免为保留接口签名而整行 eslint-disable。
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
          destructuredArrayIgnorePattern: '^_',
        },
      ],
      // 设计语言：在 className 字面量里劝阻硬编码颜色 / 非 4|6|8|12 半径。
      // 这些命中是 warn 级别，提示迁移到语义 token 或 primitive。
      'no-restricted-syntax': [
        'warn',
        {
          selector: "JSXAttribute[name.name='className'] > Literal[value=/(^|\\s)(border|bg|text|ring)-(violet|sky|emerald|orange|amber|rose|red|blue|green|yellow|fuchsia|purple|pink|indigo|cyan|teal|lime)-/]",
          message: '请使用语义 token（primary/success/warning/destructive/info）或 OriginBadge/StatusDot/Badge primitive，而非 Tailwind 调色板字面色。',
        },
        {
          selector: "JSXAttribute[name.name='className'] > Literal[value=/(^|\\s)rounded-\\[(?!(4|6|8|12)px\\])[0-9]+px\\]/]",
          message: '请使用约定的半径档位 4 / 6 / 8 / 12，而非任意数值。',
        },
        {
          selector: "JSXAttribute[name.name='className'] > Literal[value=/(^|\\s)rounded-(xl|2xl|3xl|full)(\\s|$)/]",
          message: '请优先使用 rounded-[8px]（默认 md）/ rounded-[12px]（lg），仅在极少数场景使用 rounded-full（StatusDot/Avatar 等）。',
        },
      ],
    },
  },
])
