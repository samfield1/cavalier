# Cavalier

extralive chat

## Frontend

Trunk app
```
cavalier/frontend$ trunk serve
```

## Backend

Axum app
```
cavalier/backend$ cargo run
```

## CI/CD

There are three github actions which each build a dockerfile. There is a dockerfile for the frontend, a dockerfile for the backend, and a dockerfile to build trunk and save it as a pacakge so that it doesn't have to run each time the frontend is built.
More information coming as the project develops!
