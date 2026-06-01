import 'dart:convert';
import 'dart:io';

class DartFyloRepository {
  final String root = Platform.environment['FYLO_ROOT'] ?? '${Directory.current.path}/db';
  final String? executable = Platform.environment['FYLO_EXEC_PATH'];

  Future<Map<String, Object?>> importSample(String requestId) async {
    const collection = 'language-route-events';
    final payload = Uri.encodeComponent(jsonEncode([
      {'language': 'dart', 'source': 'fylo.exec', 'requestId': requestId, 'imported': 'yes'}
    ]));
    await _machine({'op': 'createCollection', 'collection': collection});
    await _machine({
      'op': 'importBulkData',
      'collection': collection,
      'url': 'data:application/json,$payload',
      'limitOrOptions': 1,
    });
    return {
      'collection': collection,
      'operations': ['createCollection', 'importBulkData'],
      'resultCount': '2',
    };
  }

  Future<dynamic> _machine(Map<String, Object?> request) async {
    final command = executable == null || executable!.isEmpty ? 'bunx' : executable!;
    final args = executable == null || executable!.isEmpty
        ? ['--bun', 'fylo.exec', 'exec', '--request', '-', '--root', root]
        : ['exec', '--request', '-', '--root', root];
    final process = await Process.start(command, args);
    process.stdin.write(jsonEncode(request));
    await process.stdin.close();
    final stdout = await process.stdout.transform(utf8.decoder).join();
    final stderr = await process.stderr.transform(utf8.decoder).join();
    final code = await process.exitCode;
    if (code != 0) throw Exception(stderr.isNotEmpty ? stderr : stdout);
    final response = jsonDecode(stdout.isEmpty ? '{}' : stdout) as Map<String, dynamic>;
    if (response['ok'] != true) throw Exception(response['error']?['message'] ?? 'fylo.exec returned an error');
    return response['result'];
  }
}
