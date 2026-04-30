package main

func Handler(request map[string]any) any {
	return GoLanguageService{}.Describe(request)
}
