// Dark is the default; the .light class on <html> flips the token set.
const STORAGE_KEY = 'harknotes-theme';

export type Theme = 'dark' | 'light';

export function getTheme(): Theme {
  return localStorage.getItem(STORAGE_KEY) === 'light' ? 'light' : 'dark';
}

export function applyTheme(theme: Theme): void {
  document.documentElement.classList.toggle('light', theme === 'light');
}

export function setTheme(theme: Theme): void {
  localStorage.setItem(STORAGE_KEY, theme);
  applyTheme(theme);
}

export function initTheme(): void {
  applyTheme(getTheme());
}
