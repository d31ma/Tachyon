package main

import "strconv"

func Handler(request map[string]any) any {
	statusResponses := map[string]map[string]any{
		"411": {"code": "411", "detail": "length required"},
		"412": {"code": "412", "detail": "precondition failed"},
		"413": {"code": "413", "detail": "content too large"},
		"414": {"code": "414", "detail": "uri too long"},
		"415": {"code": "415", "detail": "unsupported media"},
	}
	if query, ok := request["query"].(map[string]any); ok {
		code := ""
		switch raw := query["code"].(type) {
		case string:
			code = raw
		case float64:
			code = strconv.Itoa(int(raw))
		}
		if response, ok := statusResponses[code]; ok {
			return response
		}
	}

	return GoLanguageService{}.Describe(request)
}
