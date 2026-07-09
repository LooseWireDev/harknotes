import type { Tree } from '@nx/devkit';
import { createTreeWithEmptyWorkspace } from '@nx/devkit/testing';
import { featureGenerator } from './generator';

describe('feature generator', () => {
  let tree: Tree;

  beforeEach(() => {
    tree = createTreeWithEmptyWorkspace();
    tree.write('apps/api/src/trpc.ts', 'export const appRouter = router({});');
    tree.write('apps/api/src/features/.gitkeep', '');
    tree.write('apps/web/src/features/.gitkeep', '');
    tree.write('apps/mobile/src/features/.gitkeep', '');
    tree.write('packages/shared/src/schemas/index.ts', 'export {};');
  });

  it('should create api feature files', async () => {
    await featureGenerator(tree, { name: 'users', apps: 'api' });

    expect(tree.exists('apps/api/src/features/users/router.ts')).toBeTruthy();
    expect(tree.exists('apps/api/src/features/users/service.ts')).toBeTruthy();
    expect(tree.exists('apps/api/src/features/users/service.test.ts')).toBeTruthy();
    expect(tree.exists('apps/api/src/features/users/types.ts')).toBeTruthy();
  });

  it('should create web feature files', async () => {
    await featureGenerator(tree, { name: 'users', apps: 'web' });

    expect(tree.exists('apps/web/src/features/users/UsersList.tsx')).toBeTruthy();
    expect(tree.exists('apps/web/src/features/users/useUsersActions.ts')).toBeTruthy();
    // Test file matches the component's name and casing (conventions:
    // "test files sit next to what they test").
    expect(tree.exists('apps/web/src/features/users/UsersList.test.tsx')).toBeTruthy();
    expect(tree.exists('apps/web/src/features/users/usersList.test.ts')).toBeFalsy();
  });

  it('should create mobile feature files', async () => {
    await featureGenerator(tree, { name: 'users', apps: 'mobile' });

    expect(tree.exists('apps/mobile/src/features/users/UsersList.tsx')).toBeTruthy();
    expect(tree.exists('apps/mobile/src/features/users/useUsersActions.ts')).toBeTruthy();
    expect(tree.exists('apps/mobile/src/features/users/UsersList.test.tsx')).toBeTruthy();
    expect(tree.exists('apps/mobile/src/features/users/usersList.test.ts')).toBeFalsy();
  });

  it('should handle multiple apps', async () => {
    await featureGenerator(tree, { name: 'users', apps: 'api,web,mobile' });

    expect(tree.exists('apps/api/src/features/users/router.ts')).toBeTruthy();
    expect(tree.exists('apps/web/src/features/users/UsersList.tsx')).toBeTruthy();
    expect(tree.exists('apps/mobile/src/features/users/UsersList.tsx')).toBeTruthy();
  });

  it('should add shared schema', async () => {
    await featureGenerator(tree, { name: 'users', apps: 'api' });

    expect(tree.exists('packages/shared/src/schemas/users.ts')).toBeTruthy();
  });

  it('should create desktop feature files when explicitly requested', async () => {
    tree.write('apps/desktop/src-tauri/src/lib.rs', '');

    await featureGenerator(tree, { name: 'files', apps: 'desktop' });

    expect(tree.exists('apps/desktop/src-tauri/src/commands/files.rs')).toBeTruthy();
  });

  it('should create static feature files', async () => {
    tree.write('apps/static/src/pages/.gitkeep', '');
    tree.write('apps/static/src/components/.gitkeep', '');

    await featureGenerator(tree, { name: 'users', apps: 'static' });

    expect(tree.exists('apps/static/src/pages/users/index.astro')).toBeTruthy();
    expect(tree.exists('apps/static/src/components/users/UsersIsland.tsx')).toBeTruthy();
  });

  describe('router wiring', () => {
    const ANCHORED_TRPC = [
      "import { initTRPC } from '@trpc/server';",
      '// forge:feature-imports',
      '',
      'export const appRouter = router({',
      '  // forge:feature-routers',
      '});',
      '',
    ].join('\n');

    it('wires the feature router into appRouter', async () => {
      tree.write('apps/api/src/trpc.ts', ANCHORED_TRPC);

      await featureGenerator(tree, { name: 'billing', apps: 'api' });

      const trpc = tree.read('apps/api/src/trpc.ts', 'utf-8')!;
      expect(trpc).toContain("import { billingRouter } from './features/billing/router';");
      expect(trpc).toContain('billing: billingRouter,');
    });

    it('is idempotent when run twice for the same feature', async () => {
      tree.write('apps/api/src/trpc.ts', ANCHORED_TRPC);

      await featureGenerator(tree, { name: 'billing', apps: 'api' });
      await featureGenerator(tree, { name: 'billing', apps: 'api' });

      const trpc = tree.read('apps/api/src/trpc.ts', 'utf-8')!;
      expect(trpc.match(/billing: billingRouter,/g)).toHaveLength(1);
    });

    it('leaves trpc.ts alone when the anchors are missing (pre-anchor projects)', async () => {
      tree.write('apps/api/src/trpc.ts', 'export const appRouter = router({});');

      await featureGenerator(tree, { name: 'billing', apps: 'api' });

      expect(tree.read('apps/api/src/trpc.ts', 'utf-8')).toBe(
        'export const appRouter = router({});',
      );
      expect(tree.exists('apps/api/src/features/billing/router.ts')).toBeTruthy();
    });
  });

  describe('name validation', () => {
    it('rejects names that are not camelCase identifiers', async () => {
      await expect(featureGenerator(tree, { name: 'user-profile', apps: 'api' })).rejects.toThrow(
        /Invalid feature name/,
      );
    });

    it('accepts camelCase names', async () => {
      await featureGenerator(tree, { name: 'userProfile', apps: 'api' });

      expect(tree.exists('apps/api/src/features/userProfile/router.ts')).toBeTruthy();
    });
  });
});
