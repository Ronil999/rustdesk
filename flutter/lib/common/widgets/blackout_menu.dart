import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/common/widgets/toolbar.dart';
import 'package:flutter_hbb/models/blackout_model.dart';
import 'package:flutter_hbb/models/model.dart';
import 'package:flutter_hbb/models/platform_model.dart';
import 'package:get/get.dart';

/// Control Actions -> Blackout. Shared by every controller platform.
TTextMenu blackoutMenu(String id, FFI ffi) {
  return TTextMenu(
    child: Obx(() => BlackoutMenuLabel(
          status: BlackoutState.find(id).value,
          label: translate('Blackout'),
          onText: translate('On'),
          offText: translate('Off'),
        )),
    onPressed: () => toggleBlackout(id, ffi),
  );
}

void toggleBlackout(String id, FFI ffi) {
  final state = BlackoutState.find(id);
  final supported = ffi.ffiModel.pi.features.blackout;
  final on = blackoutRequestFor(
      supported: supported,
      connected: ffi.ffiModel.waitForFirstImage.isFalse,
      status: state.value);
  if (on == null) {
    if (!supported) showToast(translate('blackout_not_supported_tip'));
    return;
  }
  state.value = on ? BlackoutStatus.turningOn : BlackoutStatus.turningOff;
  bind.sessionToggleBlackout(sessionId: ffi.sessionId, on: on);
}

void handleBlackoutEvent(Map<String, dynamic> evt, String peerId) {
  final update =
      blackoutUpdateFromResponse('${evt['status']}', evt['on'] == 'true');
  BlackoutState.find(peerId).value = update.status;
  final String details = evt['details'] ?? '';
  if (update.errorKey == null) {
    showToast(
        '${translate('Blackout')}: ${translate(update.status == BlackoutStatus.on ? 'On' : 'Off')}');
  } else {
    final text = translate(update.errorKey!);
    showToast(details.isEmpty ? text : '$text\n$details',
        timeout: const Duration(seconds: 5));
  }
}
