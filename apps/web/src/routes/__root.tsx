import { useState } from 'react';
import { createRootRoute, Link, Outlet } from '@tanstack/react-router';
import { Moon, Sun } from 'lucide-react';

import { HarknotesIcon } from '@/components/HarknotesIcon';
import { Button } from '@/components/ui/button';
import { getTheme, setTheme, type Theme } from '@/lib/theme';

export const Route = createRootRoute({
  component: RootLayout,
});

const navLink =
  'rounded-md px-2.5 py-1.5 text-sm text-text-secondary transition-colors hover:bg-surface-1 hover:text-foreground [&.active]:bg-surface-1 [&.active]:font-medium [&.active]:text-foreground';

function RootLayout(): React.ReactElement {
  const [theme, setThemeState] = useState<Theme>(getTheme());

  const toggleTheme = (): void => {
    const next = theme === 'dark' ? 'light' : 'dark';
    setTheme(next);
    setThemeState(next);
  };

  return (
    <div className="flex min-h-screen flex-col bg-background text-foreground">
      <nav className="flex items-center gap-1 border-b border-border-subtle bg-bg-elevated px-4 py-2.5">
        <Link to="/" className="mr-3 flex items-center gap-2">
          <HarknotesIcon size={26} />
          <span className="text-[15px] font-semibold tracking-tight">Harknotes</span>
        </Link>
        <Link to="/" className={navLink}>
          Meetings
        </Link>
        <Link to="/record" className={navLink}>
          Record
        </Link>
        <div className="ml-auto flex items-center gap-1">
          <Button variant="ghost" size="icon-sm" onClick={toggleTheme} title="Toggle theme">
            {theme === 'dark' ? <Sun /> : <Moon />}
          </Button>
          <Link to="/settings" className={navLink}>
            Settings
          </Link>
        </div>
      </nav>
      <main className="flex-1">
        <Outlet />
      </main>
    </div>
  );
}
