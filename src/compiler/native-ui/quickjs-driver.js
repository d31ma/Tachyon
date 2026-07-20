// @ts-check

export const QUICKJS_NG_VERSION = '0.15.1';

export function quickJSDriverHeader() {
    return `#ifndef TACHYON_UI_CONTROLLER_H
#define TACHYON_UI_CONTROLLER_H
#ifdef __cplusplus
extern "C" {
#endif
typedef struct TachyonUIController TachyonUIController;
TachyonUIController* tachyon_ui_controller_create(const char* script_path, char** error);
char* tachyon_ui_controller_render(TachyonUIController* controller, char** error);
char* tachyon_ui_controller_dispatch(TachyonUIController* controller, const char* event_json, char** error);
void tachyon_ui_controller_free_string(char* value);
void tachyon_ui_controller_destroy(TachyonUIController* controller);
#ifdef __cplusplus
}
#endif
#endif
`;
}

export function quickJSDriverSource() {
    return `#include "tachyon_ui_controller.h"
#include <quickjs.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct TachyonUIController { JSRuntime* runtime; JSContext* context; JSValue api; };

static char* duplicate_text(const char* value) {
    size_t length = value ? strlen(value) : 0;
    char* result = (char*)malloc(length + 1);
    if (!result) return NULL;
    if (length) memcpy(result, value, length);
    result[length] = 0;
    return result;
}

static void set_error(JSContext* context, JSValueConst value, char** error) {
    if (!error) return;
    const char* text = JS_ToCString(context, value);
    *error = duplicate_text(text ? text : "Unknown JavaScript controller error");
    if (text) JS_FreeCString(context, text);
}

static char* read_source(const char* path, size_t* length) {
    FILE* file = fopen(path, "rb");
    if (!file) return NULL;
    fseek(file, 0, SEEK_END); long size = ftell(file); rewind(file);
    if (size < 0) { fclose(file); return NULL; }
    char* source = (char*)malloc((size_t)size + 1);
    if (!source) { fclose(file); return NULL; }
    *length = fread(source, 1, (size_t)size, file); source[*length] = 0; fclose(file);
    return source;
}

static char* invoke(TachyonUIController* controller, const char* method, const char* argument, char** error) {
    JSContext* context = controller->context;
    JSValue function = JS_GetPropertyStr(context, controller->api, method);
    JSValue argument_value = argument ? JS_NewString(context, argument) : JS_UNDEFINED;
    JSValue result = JS_Call(context, function, controller->api, argument ? 1 : 0, argument ? &argument_value : NULL);
    JS_FreeValue(context, function); JS_FreeValue(context, argument_value);
    if (JS_IsException(result)) {
        JSValue exception = JS_GetException(context); set_error(context, exception, error);
        JS_FreeValue(context, exception); JS_FreeValue(context, result); return NULL;
    }
    if (JS_IsPromise(result)) {
        while (JS_PromiseState(context, result) == JS_PROMISE_PENDING) {
            JSContext* job_context = NULL;
            int status = JS_ExecutePendingJob(controller->runtime, &job_context);
            if (status <= 0) break;
        }
        JSPromiseStateEnum state = JS_PromiseState(context, result);
        JSValue settled = JS_PromiseResult(context, result);
        if (state != JS_PROMISE_FULFILLED) {
            set_error(context, settled, error); JS_FreeValue(context, settled); JS_FreeValue(context, result); return NULL;
        }
        JS_FreeValue(context, result); result = settled;
    }
    const char* text = JS_ToCString(context, result);
    char* copy = duplicate_text(text);
    if (text) JS_FreeCString(context, text);
    JS_FreeValue(context, result);
    return copy;
}

TachyonUIController* tachyon_ui_controller_create(const char* script_path, char** error) {
    TachyonUIController* controller = (TachyonUIController*)calloc(1, sizeof(*controller));
    if (!controller) return NULL;
    controller->api = JS_UNDEFINED;
    controller->runtime = JS_NewRuntime();
    if (!controller->runtime) { if (error) *error = duplicate_text("Unable to allocate QuickJS runtime"); free(controller); return NULL; }
    controller->context = JS_NewContext(controller->runtime);
    if (!controller->context) { if (error) *error = duplicate_text("Unable to allocate QuickJS context"); JS_FreeRuntime(controller->runtime); free(controller); return NULL; }
    size_t length = 0; char* source = read_source(script_path, &length);
    if (!source) { if (error) *error = duplicate_text("Unable to read tachyon.native-controller.js"); tachyon_ui_controller_destroy(controller); return NULL; }
    JSValue evaluated = JS_Eval(controller->context, source, length, script_path, JS_EVAL_TYPE_GLOBAL); free(source);
    if (JS_IsException(evaluated)) {
        JSValue exception = JS_GetException(controller->context); set_error(controller->context, exception, error);
        JS_FreeValue(controller->context, exception); JS_FreeValue(controller->context, evaluated); tachyon_ui_controller_destroy(controller); return NULL;
    }
    JS_FreeValue(controller->context, evaluated);
    JSValue global = JS_GetGlobalObject(controller->context);
    controller->api = JS_GetPropertyStr(controller->context, global, "__tachyonNativeUI"); JS_FreeValue(controller->context, global);
    if (JS_IsUndefined(controller->api)) { if (error) *error = duplicate_text("Native controller API is missing"); tachyon_ui_controller_destroy(controller); return NULL; }
    return controller;
}

char* tachyon_ui_controller_render(TachyonUIController* controller, char** error) { return invoke(controller, "render", NULL, error); }
char* tachyon_ui_controller_dispatch(TachyonUIController* controller, const char* event_json, char** error) { return invoke(controller, "dispatch", event_json, error); }
void tachyon_ui_controller_free_string(char* value) { free(value); }
void tachyon_ui_controller_destroy(TachyonUIController* controller) {
    if (!controller) return;
    if (controller->context) { JS_FreeValue(controller->context, controller->api); JS_FreeContext(controller->context); }
    if (controller->runtime) JS_FreeRuntime(controller->runtime);
    free(controller);
}
`;
}
