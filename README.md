# Tachyon

Tachyon is a simple to use API framework built with TypeScript (Bun). Tachyon aim to provide a simple and intuitive API framework for building serverless applications and abstracts away the complexity of configuations, letting you focus on building your application.

## Features

- Customizable methods for routes
- Use of file-system based routing
- Hot reloading of routes in development mode
- Supports dynamic routes

## Installation

```bash
npm install @vyckr/tachyon
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
- Dynamic routes should be enclosed in square brackets
- The first parameter should NOT be a dynamic route (e.g. /[version]/doc/GET)
- All dynamic routes should be within odd indexes (e.g. /v1/[path]/login/[id]/name/POST)
- The last parameter in the route should always be a capitalized method as a file name without file extension (e.g. /v1/[path]/login/[id]/name/DELETE)
- First line of the file should be a shebang for the executable file (e.g. #!/usr/bin/env python3)
- Request context can be retrieved by extracting the last element in args and parsing it.
- Response of executable script must be in a String format and must be the last value printed to output/console
- Use the exit method of the executable script with a status code to end the process of the executable script

### Examples

```typescript
// routes/v1/[collection]/doc/GET

#!/usr/bin/env bun

const ctx = JSON.parse(process.argv.pop());

console.log("Executing TypeScript....");

console.log(JSON.stringify({ message: "Hello from TypeScript!", ...ctx }));
 
process.exit(200) 
```

```python
# routes/v1/[collection]/doc/POST

#!/usr/bin/env python3
import sys
import json 

ctx = json.loads(sys.argv.pop())

print("Executing Python....")

print(json.dumps({
    "message": "Hello from Python!",
    **ctx
}))

sys.exit(200)
```

```ruby
# routes/v1/[collection]/doc/DELETE

#!/usr/bin/env ruby
require 'json'

ctx = JSON.parse(ARGV.pop)

puts "Executing Ruby...."

puts JSON.unparse(ctx.merge({
    message: "Hello from Ruby!"
}))

exit 200
```

To run the application, you can use the following command:

```bash 
tachy on
```

To invoke the API endpoints, you can use the following commands:

```bash
curl -X GET http://localhost:8000/v1/users/doc
```

```bash
curl -X POST http://localhost:8000/v1/users/doc -d '{"name": "John Doe", "age": 30}'
```

```bash
curl -X PATCH http://localhost:8000/v1/users/doc -d '{"name": "Jane Doe", "age": 31}'
```

```bash
curl -X DELETE http://localhost:8000/v1/users/doc/5e8b0a9c-c0d1-4d3b-a0b1-e2d8e0e9a1c0
```

# License

Tachyon is licensed under the MIT License.