# Common Library IDs

Use `--library-id` instead of `--library` to save 1 API call.

## JavaScript / TypeScript

| Library | Context7 ID |
|---------|-------------|
| React | `/reactjs/react.dev` |
| Next.js | `/vercel/next.js` |
| Vue | `/vuejs/docs` |
| Svelte | `/sveltejs/svelte.dev` |
| Angular | `/angular/angular` |
| Express | `/expressjs/express` |
| Fastify | `/fastify/fastify` |
| NestJS | `/nestjs/docs` |
| Prisma | `/prisma/docs` |
| Drizzle | `/drizzle-team/drizzle-orm` |
| tRPC | `/trpc/trpc` |
| Zod | `/colinhacks/zod` |
| Tailwind CSS | `/tailwindlabs/tailwindcss.com` |
| TypeScript | `/microsoft/typescript` |

## Python

| Library | Context7 ID |
|---------|-------------|
| FastAPI | `/tiangolo/fastapi` |
| Django | `/django/docs` |
| Flask | `/pallets/flask` |
| SQLAlchemy | `/sqlalchemy/sqlalchemy` |
| Pydantic | `/pydantic/pydantic` |
| LangChain | `/langchain-ai/langchain` |

## Databases

| Library | Context7 ID |
|---------|-------------|
| MongoDB | `/mongodb/docs` |
| PostgreSQL | `/postgres/postgres` |
| Redis | `/redis/redis-doc` |
| Supabase | `/supabase/supabase` |

## Cloud / DevOps

| Library | Context7 ID |
|---------|-------------|
| Docker | `/docker/docs` |
| Kubernetes | `/kubernetes/website` |
| Terraform | `/hashicorp/terraform` |
| AWS CDK | `/aws/aws-cdk` |

## Finding New Library IDs

If a library isn't listed, use `--library <name>` and the skill will resolve it. Check the verbose output for the resolved ID to use next time:

```bash
bash scripts/fetch-docs.sh --library "your-library" --topic "getting started" --verbose
```
