import { joinPathFragments, type Tree } from '@nx/devkit';
import type { FeatureGeneratorSchema } from './schema';

// Feature names become identifiers (`${name}Router`) and directory names,
// so they must be valid camelCase identifiers.
const FEATURE_NAME_RE = /^[a-z][a-zA-Z0-9]*$/;

export async function featureGenerator(tree: Tree, options: FeatureGeneratorSchema): Promise<void> {
  const projectRoot = options.targetDir ?? '.';
  const name = options.name;
  if (!FEATURE_NAME_RE.test(name)) {
    throw new Error(
      `Invalid feature name "${name}". Use a camelCase identifier starting with a lowercase letter (e.g. "userProfile").`,
    );
  }
  const className = name.charAt(0).toUpperCase() + name.slice(1);
  const apps = options.apps.split(',').map((a) => a.trim());

  for (const app of apps) {
    switch (app) {
      case 'api':
        generateApiFeature(tree, projectRoot, name);
        wireApiRouter(tree, projectRoot, name);
        break;
      case 'web':
        generateWebFeature(tree, projectRoot, name, className);
        break;
      case 'mobile':
        generateMobileFeature(tree, projectRoot, name, className);
        break;
      case 'desktop':
        generateDesktopFeature(tree, projectRoot, name);
        break;
      case 'static':
        generateStaticFeature(tree, projectRoot, name, className);
        break;
    }
  }

  // Add AGENTS.md stubs per feature directory for agent context
  for (const app of apps) {
    const featureDir = getFeatureDir(projectRoot, app, name);
    if (featureDir) {
      tree.write(
        joinPathFragments(featureDir, 'AGENTS.md'),
        `# ${className} Feature (${app})

## Summary
<!-- What this feature does — filled in after implementation -->

## Business Rules
<!-- Key constraints and invariants -->

## Schemas
<!-- Zod schemas and their purpose -->

## Relations
<!-- How this feature connects to other features -->
`,
      );
    }
  }

  // Add shared schema
  const sharedSchemaPath = joinPathFragments(projectRoot, `packages/shared/src/schemas/${name}.ts`);
  tree.write(
    sharedSchemaPath,
    `import { z } from 'zod';

// Shared schemas for the ${name} feature
// These are consumed by both API and client apps.

export {};
`,
  );
}

/**
 * Register the feature router in appRouter via the anchor comments the api
 * generator leaves in trpc.ts. Missing file or anchors means a python
 * backend or a pre-anchor project — the feature files still land, wiring
 * is the developer's job there.
 */
function wireApiRouter(tree: Tree, root: string, name: string): void {
  const trpcPath = joinPathFragments(root, 'apps/api/src/trpc.ts');
  const content = tree.read(trpcPath, 'utf-8') ?? '';
  if (
    !content.includes('// forge:feature-imports') ||
    !content.includes('// forge:feature-routers')
  ) {
    return;
  }
  const importLine = `import { ${name}Router } from './features/${name}/router';`;
  if (content.includes(importLine)) return;
  const updated = content
    .replace('// forge:feature-imports', `${importLine}\n// forge:feature-imports`)
    .replace('// forge:feature-routers', `${name}: ${name}Router,\n  // forge:feature-routers`);
  tree.write(trpcPath, updated);
}

function generateApiFeature(tree: Tree, root: string, name: string): void {
  const dir = joinPathFragments(root, `apps/api/src/features/${name}`);

  tree.write(
    joinPathFragments(dir, 'router.ts'),
    `import { router, publicProcedure } from '../../trpc';
import { z } from 'zod';

export const ${name}Router = router({
  // Add procedures here
});
`,
  );

  tree.write(
    joinPathFragments(dir, 'service.ts'),
    `// Business logic for ${name} feature
// Add service functions here. Each must have an explicit return type.

export {};
`,
  );

  tree.write(
    joinPathFragments(dir, 'service.test.ts'),
    `import { describe, it, expect } from 'vitest';

describe('${name} service', () => {
  // Write tests from spec before implementation
  it.todo('should be implemented based on spec');
});
`,
  );

  tree.write(
    joinPathFragments(dir, 'types.ts'),
    `import { z } from 'zod';

// Feature-specific Zod schemas for ${name}
// Add input/output schemas here.

export {};
`,
  );
}

function generateWebFeature(tree: Tree, root: string, name: string, className: string): void {
  const dir = joinPathFragments(root, `apps/web/src/features/${name}`);

  tree.write(
    joinPathFragments(dir, `${className}List.tsx`),
    `import { trpc } from '../../lib/trpc';

export default function ${className}List(): React.ReactElement {
  // const { data } = trpc.${name}.useQuery();

  return (
    <div>
      <h1>${className}</h1>
      {/* Implement UI here */}
    </div>
  );
}
`,
  );

  tree.write(
    joinPathFragments(dir, `use${className}Actions.ts`),
    `// Hook for ${name} feature actions

export function use${className}Actions(): void {
  // TODO: implement actions
}
`,
  );

  tree.write(
    joinPathFragments(dir, `${className}List.test.tsx`),
    `import { describe, it, expect } from 'vitest';

describe('${className}List', () => {
  it.todo('should be implemented based on spec');
});
`,
  );
}

function generateMobileFeature(tree: Tree, root: string, name: string, className: string): void {
  const dir = joinPathFragments(root, `apps/mobile/src/features/${name}`);

  tree.write(
    joinPathFragments(dir, `${className}List.tsx`),
    `import React from 'react';
import { View, Text } from 'react-native';

export default function ${className}List(): React.ReactElement {
  return (
    <View>
      <Text>${className}</Text>
    </View>
  );
}
`,
  );

  tree.write(
    joinPathFragments(dir, `use${className}Actions.ts`),
    `// Hook for ${name} feature actions

export function use${className}Actions(): void {
  // TODO: implement actions
}
`,
  );

  tree.write(
    joinPathFragments(dir, `${className}List.test.tsx`),
    `import { describe, it, expect } from 'vitest';

describe('${className}List', () => {
  it.todo('should be implemented based on spec');
});
`,
  );
}

function generateDesktopFeature(tree: Tree, root: string, name: string): void {
  const commandPath = joinPathFragments(root, `apps/desktop/src-tauri/src/commands/${name}.rs`);
  tree.write(
    commandPath,
    `// Tauri Rust commands for ${name} feature
// Add OS-access commands here (file system, notifications, etc.)

#[tauri::command]
pub fn ${name}_command() -> String {
    format!("${name} command executed")
}
`,
  );
}

function generateStaticFeature(tree: Tree, root: string, name: string, className: string): void {
  tree.write(
    joinPathFragments(root, `apps/static/src/pages/${name}/index.astro`),
    `---
import Layout from '../../layouts/Layout.astro';
import ${className}Island from '../../components/${name}/${className}Island';
---
<Layout title="${className}">
  <h1>${className}</h1>
  <${className}Island client:load />
</Layout>
`,
  );

  tree.write(
    joinPathFragments(root, `apps/static/src/components/${name}/${className}Island.tsx`),
    `import React from 'react';

export default function ${className}Island(): React.ReactElement {
  return (
    <div>
      <p>${className} interactive component</p>
    </div>
  );
}
`,
  );
}

function getFeatureDir(root: string, app: string, name: string): string | null {
  switch (app) {
    case 'api':
      return joinPathFragments(root, `apps/api/src/features/${name}`);
    case 'web':
      return joinPathFragments(root, `apps/web/src/features/${name}`);
    case 'mobile':
      return joinPathFragments(root, `apps/mobile/src/features/${name}`);
    case 'static':
      return joinPathFragments(root, `apps/static/src/pages/${name}`);
    default:
      return null;
  }
}

export default featureGenerator;
