---
kind: skill
title: Zustand 스토어(TypeScript)
title_en: Zustand store (TypeScript)
summary: TypeScript로 Zustand 스토어를 정해진 틀대로 만들어요
summary_en: Creates typed Zustand stores from a set pattern
detail: React 전역 상태를 Zustand로 만들 때 쓰여요. 상태와 액션 타입을 나누고 subscribeWithSelector 미들웨어를 쓰는 템플릿으로 스토어를 만들고, 컴포넌트에서는 개별 셀렉터로 읽게 해요.
detail_en: Used when building React global state with Zustand: creates stores from a template that separates state and action types and uses subscribeWithSelector, and reads them with individual selectors.
category: React
category_en: React
name: zustand-store-ts
description: "Create Zustand stores with TypeScript, subscribeWithSelector middleware, and proper state/action separation. Use when building React state management, creating global stores, or implementing reactive state patterns with Zustand."
pinned: true
license: MIT
copyright: "Copyright (c) Microsoft Corporation."
source_repo: microsoft/skills
source_commit: 91ec09549ca16bd6c44e6ccdef26d61eb2082f07
source_path: .github/plugins/azure-sdk-typescript/skills/zustand-store-ts/SKILL.md
license_path: LICENSE
---
# Zustand Store

Create Zustand stores following established patterns with proper TypeScript types and middleware.

## Quick Start

Copy the template below and replace placeholders:
- `{{StoreName}}` → PascalCase store name (e.g., `Project`)
- `{{description}}` → Brief description for JSDoc

````typescript
import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';

// ============================================================================
// Types
// ============================================================================

/**
 * {{StoreName}}State - The state shape for this store
 */
export interface {{StoreName}}State {
  // Add state properties
  items: unknown[];
  selectedId: string | null;
  isLoading: boolean;
  error: string | null;
}

/**
 * {{StoreName}}Actions - Actions that can be performed on this store
 */
export interface {{StoreName}}Actions {
  // Setters
  setItems: (items: unknown[]) => void;
  setSelectedId: (id: string | null) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;

  // Complex actions
  loadItems: () => Promise<void>;
  addItem: (item: unknown) => void;
  removeItem: (id: string) => void;

  // Reset
  reset: () => void;
}

/**
 * {{StoreName}}Store - Combined store type
 */
export type {{StoreName}}Store = {{StoreName}}State & {{StoreName}}Actions;

// ============================================================================
// Initial State
// ============================================================================

const initialState: {{StoreName}}State = {
  items: [],
  selectedId: null,
  isLoading: false,
  error: null,
};

// ============================================================================
// Store
// ============================================================================

/**
 * use{{StoreName}}Store - Zustand store for managing {{description}}
 *
 * @example
 * ```typescript
 * // In a component - use individual selectors for performance
 * const items = use{{StoreName}}Store((state) => state.items);
 * const loadItems = use{{StoreName}}Store((state) => state.loadItems);
 *
 * // Subscribe to changes outside React
 * use{{StoreName}}Store.subscribe(
 *   (state) => state.selectedId,
 *   (selectedId) => console.log('Selected:', selectedId)
 * );
 * ```
 */
export const use{{StoreName}}Store = create<{{StoreName}}Store>()(
  subscribeWithSelector((set, get) => ({
    // Initial state
    ...initialState,

    // Simple setters
    setItems: (items) => set({ items }),
    setSelectedId: (selectedId) => set({ selectedId }),
    setLoading: (isLoading) => set({ isLoading }),
    setError: (error) => set({ error }),

    // Async action example
    loadItems: async () => {
      set({ isLoading: true, error: null });
      try {
        // const items = await fetchItems();
        const items: unknown[] = []; // Replace with actual fetch
        set({ items, isLoading: false });
      } catch (error) {
        set({
          error: error instanceof Error ? error.message : 'Failed to load',
          isLoading: false,
        });
      }
    },

    // Add item (immutable update)
    addItem: (item) => {
      set({ items: [...get().items, item] });
    },

    // Remove item (immutable update)
    removeItem: (id) => {
      set({
        items: get().items.filter((item) => (item as { id: string }).id !== id),
        // Clear selection if removed item was selected
        selectedId: get().selectedId === id ? null : get().selectedId,
      });
    },

    // Reset to initial state
    reset: () => set(initialState),
  }))
);
````

## Always Use subscribeWithSelector

```typescript
import { create } from 'zustand';
import { subscribeWithSelector } from 'zustand/middleware';

export const useMyStore = create<MyStore>()(
  subscribeWithSelector((set, get) => ({
    // state and actions
  }))
);
```

## Separate State and Actions

```typescript
export interface MyState {
  items: Item[];
  isLoading: boolean;
}

export interface MyActions {
  addItem: (item: Item) => void;
  loadItems: () => Promise<void>;
}

export type MyStore = MyState & MyActions;
```

## Use Individual Selectors

```typescript
// Good - only re-renders when `items` changes
const items = useMyStore((state) => state.items);

// Avoid - re-renders on any state change
const { items, isLoading } = useMyStore();
```

## Subscribe Outside React

```typescript
useMyStore.subscribe(
  (state) => state.selectedId,
  (selectedId) => console.log('Selected:', selectedId)
);
```

## Integration Steps

1. Create store in `src/frontend/src/store/`
2. Export from `src/frontend/src/store/index.ts`
3. Add tests in `src/frontend/src/store/*.test.ts`
