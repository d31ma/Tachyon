class DartGallery extends Tac {
  String status = 'checking';
  String mathNote = 'not computed';
  String docCount = 'press Seed FYLO to write a document';
  String lastWave = 'none yet';

  @onMount()
  Future<void> ready() async {
    await localStorage.setItem('tachyon.language.dart', 'ready');
    final result = await fylo.collection('tac-companion-probes').find({ 'limit': 1 });
    final storage = await localStorage.getItem('tachyon.language.dart', 'storage unavailable');
    status = '$storage - FYLO ${result['local'] == true ? 'local' : 'ready'}';
    final ratio = 7 ~/ 2;
    mathNote = '7 ~/ 2 = $ratio (real Dart integer division)';
  }

  Future<void> seed() async {
    await fylo.collection('tac-companion-probes').create({ 'label': 'dart', 'compiledBy': 'dart compile js' });
    final result = await fylo.collection('tac-companion-probes').find({});
    docCount = '${result['docs'].length} documents in OPFS';
  }

  void wave() {
    this.publish('tachyon:wave', 'Dart');
  }

  @subscribe('tachyon:wave')
  void onWave(dynamic language) {
    lastWave = '$language';
  }
}
