---
kind: rule
title: FastAPI 규칙
title_en: FastAPI rules
summary: FastAPI 라우터·모델·비동기 관례를 적용해요
summary_en: FastAPI routers, models and async
detail: .py 파일을 다룰 때만 적용돼요. Pydantic 모델, 라우터 분리, 의존성 주입, 비동기 처리, 오류 응답 같은 FastAPI 모범 사례를 따르게 해요.
detail_en: Applies only to .py files: Pydantic models, split routers, dependency injection, async handling and error responses, FastAPI-style.
category: Python
category_en: Python
name: fastapi
description: "FastAPI best practices and patterns for building modern Python web APIs"
paths:
  - "**/*.py"
adapt: cursorrules
license: CC0-1.0
copyright: "CC0 1.0 Universal (public domain dedication), PatrickJS and contributors"
source_repo: PatrickJS/awesome-cursorrules
source_commit: b044f956f021b6e8877f16781bcfc466a6a120e9
source_path: rules/fastapi.mdc
license_path: LICENSE
tokens: 0
performance: 0
quality: 0
---
# FastAPI Best Practices

## Project Structure
- Use proper directory structure
- Implement proper module organization
- Use proper dependency injection
- Keep routes organized by domain
- Implement proper middleware
- Use proper configuration management

## API Design
- Use proper HTTP methods
- Implement proper status codes
- Use proper request/response models
- Implement proper validation
- Use proper error handling
- Document APIs with OpenAPI

## Models
- Use Pydantic models
- Implement proper validation
- Use proper type hints
- Keep models organized
- Use proper inheritance
- Implement proper serialization

## Database
- Use proper ORM (SQLAlchemy)
- Implement proper migrations
- Use proper connection pooling
- Implement proper transactions
- Use proper query optimization
- Handle database errors properly

## Authentication
- Implement proper JWT authentication
- Use proper password hashing
- Implement proper role-based access
- Use proper session management
- Implement proper OAuth2
- Handle authentication errors properly

## Security
- Implement proper CORS
- Use proper rate limiting
- Implement proper input validation
- Use proper security headers
- Handle security errors properly
- Implement proper logging

## Performance
- Use proper caching
- Implement proper async operations
- Use proper background tasks
- Implement proper connection pooling
- Use proper query optimization
- Monitor performance metrics

## Testing
- Write proper unit tests
- Implement proper integration tests
- Use proper test fixtures
- Implement proper mocking
- Test error scenarios
- Use proper test coverage

## Deployment
- Use proper Docker configuration
- Implement proper CI/CD
- Use proper environment variables
- Implement proper logging
- Use proper monitoring
- Handle deployment errors properly

## Documentation
- Use proper docstrings
- Implement proper API documentation
- Use proper type hints
- Keep documentation updated
- Document error scenarios
- Use proper versioning
