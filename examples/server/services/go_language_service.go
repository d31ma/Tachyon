package main

type GoLanguageService struct{}

func (service GoLanguageService) Describe(request map[string]any) any {
	context, _ := request["context"].(map[string]any)
	requestID, _ := context["requestId"].(string)
	if requestID == "" {
		requestID = "unknown"
	}
	return map[string]any{
		"language":  "go",
		"message":   "Hello from Go!",
		"requestId": requestID,
	}
}
