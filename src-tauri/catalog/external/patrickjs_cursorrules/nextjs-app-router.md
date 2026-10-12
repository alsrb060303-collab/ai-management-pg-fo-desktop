---
kind: rule
title: Next.js App Router 규칙
title_en: Next.js App Router rules
summary: App Router 구조·서버 컴포넌트 규칙을 적용해요
summary_en: App Router structure and server components
detail: Next.js의 app 폴더 파일을 다룰 때만 적용돼요. 서버 컴포넌트 우선, 라우트·레이아웃 구조, 데이터 가져오기, 메타데이터 같은 App Router 관례를 따르게 해요.
detail_en: Applies only to files under Next.js's app folder: server components first, route and layout structure, data fetching and metadata conventions.
category: Next.js
category_en: Next.js
name: nextjs-app-router
description: "Cursor rules for Next.js development with App Router integration."
paths:
  - "app/**/*.ts"
  - "app/**/*.tsx"
  - "src/app/**/*.ts"
  - "src/app/**/*.tsx"
adapt: cursorrules
license: CC0-1.0
copyright: "CC0 1.0 Universal (public domain dedication), PatrickJS and contributors"
source_repo: PatrickJS/awesome-cursorrules
source_commit: b044f956f021b6e8877f16781bcfc466a6a120e9
source_path: rules/nextjs-app-router-cursorrules-prompt-file.mdc
license_path: LICENSE
---
// Next.js App Router .cursorrules

// Next.js App Router best practices

const nextjsAppRouterBestPractices = [
  "Use server components by default",
  "Implement client components only when necessary",
  "Utilize the new file-based routing system",
  "Use layout.js for shared layouts",
  "Implement loading.js for loading states",
  "Use error.js for error handling",
  "Utilize route handlers for API routes",
];

// Folder structure

const folderStructure = `
app/
  layout.js
  page.js
  components/
  lib/
  styles/
public/
`;

// Additional instructions

const additionalInstructions = `
1. Use TypeScript for type safety
2. Implement proper metadata for SEO
3. Utilize Next.js Image component for optimized images
4. Use CSS Modules or Tailwind CSS for styling
5. Implement proper error boundaries
6. Follow Next.js naming conventions for special files
7. Use environment variables for configuration
`;
