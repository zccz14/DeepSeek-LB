import { readdirSync, readFileSync } from 'node:fs';

const css = readdirSync(new URL('../dist/assets/', import.meta.url))
  .filter((file) => file.endsWith('.css'))
  .map((file) => readFileSync(new URL(`../dist/assets/${file}`, import.meta.url), 'utf8'))
  .join('\n');
for (const selector of ['.linkit-user-picker{', '.linkit-user-picker__input{', '.linkit-user-picker__results{', '.linkit-user-picker__option{']) {
  if (!css.includes(selector)) throw new Error(`built DeepSeek-LB CSS is missing LinkitUserPicker selector ${selector}`);
}
