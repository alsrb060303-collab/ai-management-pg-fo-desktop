---
kind: rule
title: Docker 규칙
title_en: Docker rules
summary: 버전 고정·멀티 스테이지·root 금지로 컨테이너를 만들어요
summary_en: Pinned, multi-stage, non-root images
detail: Dockerfile·docker-compose 파일을 다룰 때만 적용돼요. 베이스 이미지 버전 고정, 멀티 스테이지 빌드, root가 아닌 사용자, 작은 이미지 같은 운영용 규칙을 따르게 해요.
detail_en: Applies only to Dockerfiles and compose files: pinned base images, multi-stage builds, a non-root user and small images.
category: 인프라
category_en: Infrastructure
name: docker
description: "Docker production rules. Pinned versions, multi-stage builds, non-root user, minimal attack surface."
paths:
  - "**/Dockerfile"
  - "**/Dockerfile.*"
  - "**/docker-compose*.yml"
  - "**/docker-compose*.yaml"
  - "**/.dockerignore"
adapt: cursorrules
license: CC0-1.0
copyright: "CC0 1.0 Universal (public domain dedication), PatrickJS and contributors"
source_repo: PatrickJS/awesome-cursorrules
source_commit: b044f956f021b6e8877f16781bcfc466a6a120e9
source_path: rules/docker.mdc
license_path: LICENSE
---
# Docker Rules

Expert Docker practitioner. Minimal, secure, reproducible images.

## Dockerfile
- Pin versions: FROM node:20.11-alpine3.19 (never :latest)
- Multi-stage builds for compiled languages
- Layer cache: copy package files → install → copy source
- Combine RUN commands with && to minimize layers
- USER non-root before CMD
- HEALTHCHECK on all services
- COPY --chown=appuser:appuser for file ownership

## Security
- Never run as root
- No secrets in Dockerfile or image layers
- No .env files copied into image
- Scan with docker scout or trivy in CI

## .dockerignore
- Always present: node_modules, .git, *.log, .env*, test files

## Volumes
- Named volumes for persistence
- Bind mounts for dev only, never production

## Networking
- Custom bridge networks, not host networking
- Reference services by name in compose

## Logging
- Always stdout/stderr — never log to files inside container

## Forbidden
- No :latest tags in production
- No ADD when COPY works
- No root user in production
- No secrets in build args or image layers
