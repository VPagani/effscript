import { Effect } from "effect"
import { program } from "./app.efs"

await Effect.runPromise(program)
