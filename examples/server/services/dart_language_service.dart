import 'dart_fylo_repository.dart';

class DartLanguageService {
  final DartFyloRepository _fyloRepository = DartFyloRepository();

  Future<Map<String, Object?>> delete(Map<String, dynamic> request) async {
    final context = request['context'];
    final requestId = context is Map ? context['requestId']?.toString() ?? 'unknown' : 'unknown';
    return {
      'message': 'Hello from Dart!',
      'requestId': requestId,
      'fylo': await _fyloRepository.importSample(requestId),
    };
  }
}
