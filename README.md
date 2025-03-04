# Tachyon

Tachyon is a simple to use API framework built with TypeScript (Bun). Tachyon aim to provide a simple and intuitive API framework for building serverless applications and abstracts away the complexity of configuations, letting you focus on building your application.

## Features

- Customizable methods for routes
- Use of file-system based routing
- Hot reloading of routes in development mode
- Supports dynamic routes

## Installation

```bash
bun add @vyckr/tachyon
```

## Configuration

The .env file should be in the root directory of your project. The following environment variables:
```
# Tachyon environment variables
PORT=8000 (optional)
NODE_ENV=development|production (optional)
HOSTNAME=127.0.0.1 (optional)
ALLOW_HEADERS=* (optional)
ALLOW_ORGINS=* (optional)
ALLOW_CREDENTIALS=true|false (optional)
ALLOW_EXPOSE_HEADERS=* (optional)
ALLOW_MAX_AGE=3600 (optional)
ALLOW_METHODS=GET,POST,PUT,DELETE,PATCH (optional)
```

### Requirements
- Make sure to have a 'routes' directory in the root of your project
- Dynamic routes should start with a colon `:`
- The first parameter should NOT be a dynamic route (e.g. /:version/doc/GET)
- All dynamic routes should be within odd indexes (e.g. /v1/:path/login/:id/POST)
- The last parameter in the route should always be a capitalized method as a file name without file extension (e.g. /v1/:path/login/:id/name/DELETE)
- Front-end Pages end with capitalized `HTML` filename (e.g. /v1/HTML)
- Node modules should be imported dynamically with `modules` prefix (e.g. const { default: dayjs } = await import(`/modules/dayjs.js`))
- Components should be in the `components` folder and end with `.html` extension (e.g. /components/counter.html)
- First line of the file should be a shebang for the executable file (e.g. #!/usr/bin/env python3)
- Request context can be retrieved by extracting the last element in args and parsing it.
- Response of executable script must be in a String format and must written to the `/tmp` folder with the the process ID as the file name (e.g. `/tmp/1234`).
- Use the exit method of the executable script with a status code to end the process of the executable script

### Examples


```html
<!-- /routes/HTML  -->
<script>
    // top-level await
    const { default: dayjs } = await import("/modules/dayjs.js")

    console.log(dayjs().format())

    const greeting = "Hello World!"
</script>

<p>${greeting}</p>
```

```typescript
// routes/v1/:collection/GET

#!/usr/bin/env bun

for await(const chunk of Bun.stdin.stream()) {

    console.log("Executing Bun....");

    const data = new TextDecoder().decode(chunk)

    const ctx = JSON.parse(data)

    ctx.message = "Hello from Bun!"

    const response = JSON.stringify(ctx)

    await Bun.write(`/tmp/${process.pid}`, response)
}
```

```python
# routes/v1/:collection/POST

#!/usr/bin/env python3
import json
import sys
import os

print("Executing Python....")

ctx = json.loads(sys.stdin.read())

ctx["message"] = "Hello from Python!"

file = open(f"/tmp/{os.getpid()}", "w")

file.write(json.dumps(ctx))

file.close()
```

```ruby
# routes/v1/:collection/DELETE

#!/usr/bin/env ruby
require 'json'

puts "Executing Ruby...."

ctx = JSON.parse(ARGF.read)

ctx["message"] = "Hello from Ruby!"

File.write("/tmp/#{Process.pid}", JSON.unparse(ctx))
```

To run the application, you can use the following command:

```bash 
bun tach
```

To invoke the API endpoints, you can use the following commands:

```bash
curl -X GET http://localhost:8000/v1/users
```

```bash
curl -X POST http://localhost:8000/v1/users -d '{"name": "John Doe", "age": 30}'
```

```bash
curl -X PATCH http://localhost:8000/v1/users -d '{"name": "Jane Doe", "age": 31}'
```

```bash
curl -X DELETE http://localhost:8000/v1/users/5e8b0a9c-c0d1-4d3b-a0b1-e2d8e0e9a1c0
```

To to build front-end assets into a `dist` folder, use the following command:

```bash 
bun yon
```

# License

Tachyon is licensed under the MIT License.