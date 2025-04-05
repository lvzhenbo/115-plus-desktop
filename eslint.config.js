import js from '@eslint/js';
import pluginVue from 'eslint-plugin-vue';
import {
  defineConfigWithVueTs,
  vueTsConfigs,
  configureVueProject,
} from '@vue/eslint-config-typescript';
import prettierConfig from '@vue/eslint-config-prettier';
import parserVue from 'vue-eslint-parser';
import markdown from '@eslint/markdown';
import AutoImport from './.eslintrc-auto-import.json' with { type: 'json' };

configureVueProject({
  tsSyntaxInTemplates: true,
  scriptLangs: ['ts', 'tsx'],
});

export default defineConfigWithVueTs(
  {
    name: 'app/all-js&ts-files',
    files: ['**/*.{js,mjs,cjs,jsx,ts,mts,tsx,vue}'],
    rules: {
      ...js.configs.recommended.rules,
    },
  },
  vueTsConfigs.recommended,
  {
    extends: [...pluginVue.configs['flat/recommended']],
    name: 'app/vue-files',
    files: ['**/*.vue'],
    languageOptions: {
      parser: parserVue,
      parserOptions: {
        ecmaVersion: 'latest',
      },
    },
  },
  {
    name: 'app/files-to-lint',
    files: ['**/*.{ts,mts,tsx,vue}'],
    languageOptions: {
      globals: {
        ...AutoImport.globals,
      },
    },
    rules: {
      'no-empty': 'off',
      // 允许被禁止的类型 https://typescript-eslint.io/rules/ban-types/
      '@typescript-eslint/ban-types': 'off',
      // 允许any类型 https://typescript-eslint.io/rules/no-explicit-any/
      '@typescript-eslint/no-explicit-any': 'off',
      // 允许任何TS指令注释 https://typescript-eslint.io/rules/ban-ts-comment
      '@typescript-eslint/ban-ts-comment': 'off',
      // 允许空函数 https://typescript-eslint.io/rules/no-empty-function/
      '@typescript-eslint/no-empty-function': 'off',
      '@typescript-eslint/no-unused-vars': [
        'warn',
        {
          args: 'all',
          argsIgnorePattern: '^_',
          caughtErrors: 'all',
          caughtErrorsIgnorePattern: '^_',
          destructuredArrayIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          ignoreRestSiblings: true,
        },
      ],
      // 关闭强制多词组件名 https://eslint.vuejs.org/rules/multi-word-component-names.html
      'vue/multi-word-component-names': 'off',
      // 强制执行自我关闭风格 https://eslint.vuejs.org/rules/html-self-closing.html
      'vue/html-self-closing': [
        'error',
        {
          html: {
            void: 'always',
            normal: 'never',
            component: 'always',
          },
          svg: 'always',
          math: 'always',
        },
      ],
    },
  },
  {
    name: 'app/markdown-files',
    files: ['**/*.md'],
    plugins: {
      markdown,
    },
    language: 'markdown/gfm',
    rules: {
      'markdown/fenced-code-language': 'error',
      'markdown/heading-increment': 'error',
      'markdown/no-empty-links': 'error',
      'markdown/no-invalid-label-refs': 'error',
      'markdown/no-missing-label-refs': 'error',
    },
  },
  prettierConfig,
  {
    name: 'app/files-to-ignore',
    ignores: ['**/dist/**', '**/dist-ssr/**', '**/coverage/**'],
  },
);
