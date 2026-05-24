import 'dart_language_service.dart';

Future<Map<String, Object?>> handler(Map<String, dynamic> request) async {
  final responses = <String, Map<String, Object?>>{
    '429': {'code': '429', 'detail': 'too many requests'},
    '431': {'code': '431', 'detail': 'header too large'},
    '451': {'code': '451', 'detail': 'unavailable legal'},
    '500': {'code': '500', 'detail': 'internal error'},
    '501': {'code': '501', 'detail': 'not implemented'},
  };
  final query = request['query'];
  final raw = query is Map ? query['code'] : null;
  final code = raw is num ? raw.floor().toString() : (raw?.toString() ?? '');
  final response = responses[code];
  if (response != null) {
    return response;
  }

  return await DartLanguageService().delete(request);
}
