---
kind: rule
title: HTMX + Django 규칙
title_en: HTMX + Django rules
summary: Django 템플릿에 HTMX를 쓰는 관례를 적용해요
summary_en: HTMX patterns in Django templates
detail: Django의 .py 파일과 템플릿을 다룰 때만 적용돼요. 부분 템플릿 응답, hx 속성 사용, 뷰 구성 같은 HTMX + Django 패턴을 따르게 해요.
detail_en: Applies only to Django .py files and templates: partial template responses, hx attributes and view structure.
category: Python
category_en: Python
name: htmx-django
description: "Cursor rules for HTMX development with Django integration."
paths:
  - "**/*.py"
  - "**/templates/**/*.html"
adapt: cursorrules
license: CC0-1.0
copyright: "CC0 1.0 Universal (public domain dedication), PatrickJS and contributors"
source_repo: PatrickJS/awesome-cursorrules
source_commit: b044f956f021b6e8877f16781bcfc466a6a120e9
source_path: rules/htmx-django-cursorrules-prompt-file.mdc
license_path: LICENSE
tokens: 0
performance: 0
quality: 0
---
// HTMX with Django .cursorrules

// HTMX and Django best practices

const htmxDjangoBestPractices = [
  "Use Django's template system with HTMX attributes",
  "Implement Django forms for form handling",
  "Utilize Django's URL routing system",
  "Use Django's class-based views for HTMX responses",
  "Implement Django ORM for database operations",
  "Utilize Django's middleware for request/response processing",
];

// Folder structure

const folderStructure = `
project_name/
  app_name/
    templates/
    static/
      css/
      js/
    models.py
    views.py
    urls.py
  project_name/
    settings.py
    urls.py
manage.py
`;

// Additional instructions

const additionalInstructions = `
1. Use Django's template tags with HTMX attributes
2. Implement proper CSRF protection with Django's built-in features
3. Utilize Django's HttpResponse for HTMX-specific responses
4. Use Django's form validation for HTMX requests
5. Implement proper error handling and logging
6. Follow Django's best practices for project structure
7. Use Django's staticfiles app for managing static assets
`;
