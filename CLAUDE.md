## Agent Instructions

## Architecture

- Backend is written in Rust and uses Actix Web for the web framework.

- Frontend is written in TypeScript and uses React for the web framework.

- Database is PostgreSQL with pgvector extension.

- AI is handled by a Rust seeder that downloads and runs a local ONNX model.

## TDD

- Always use Test Driven Development. Write tests first, ensure that they fail, and then write the code to make them pass.

- Ensure your tests are comprehensive and account for edge cases.

- If tests fail, continue iterating until they pass.

## General Guidelines

- Functions should have a single responsibility, if you have to leave a comment describing what the function does, it is too complicated and you should break it into smaller tasks.

- Files should not exceed 400 lines of code. If it is approaching that limit, refactor into smaller files.

