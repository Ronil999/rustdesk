import 'package:flutter/material.dart';
import 'package:get/get.dart';

enum BlackoutStatus { off, turningOn, on, turningOff }

extension BlackoutStatusX on BlackoutStatus {
  bool get isPending =>
      this == BlackoutStatus.turningOn || this == BlackoutStatus.turningOff;
}

class BlackoutUpdate {
  final BlackoutStatus status;

  /// Lang key of the error to show, null on success.
  final String? errorKey;
  const BlackoutUpdate(this.status, this.errorKey);
}

/// The host's reply decides what is shown, so a request the host could not
/// carry out never displays ON.
BlackoutUpdate blackoutUpdateFromResponse(String status, bool on) {
  final next = on ? BlackoutStatus.on : BlackoutStatus.off;
  switch (status) {
    case 'ok':
      return BlackoutUpdate(next, null);
    case 'not_supported':
      return BlackoutUpdate(next, 'blackout_not_supported_tip');
    case 'denied':
      return BlackoutUpdate(next, 'blackout_denied_tip');
    default:
      return BlackoutUpdate(next, 'blackout_failed_tip');
  }
}

/// The `on` value a click sends, or null when no request may be sent now.
bool? blackoutRequestFor(
    {required bool supported,
    required bool connected,
    required BlackoutStatus status}) {
  if (!supported || !connected || status.isPending) return null;
  return status == BlackoutStatus.off;
}

class BlackoutState {
  static String tag(String id) => 'blackout_$id';

  static Rx<BlackoutStatus> find(String id) {
    final key = tag(id);
    if (!Get.isRegistered<Rx<BlackoutStatus>>(tag: key)) {
      Get.put<Rx<BlackoutStatus>>(BlackoutStatus.off.obs, tag: key);
    }
    return Get.find<Rx<BlackoutStatus>>(tag: key);
  }

  static void reset(String id) => find(id).value = BlackoutStatus.off;

  static void delete(String id) {
    final key = tag(id);
    if (Get.isRegistered<Rx<BlackoutStatus>>(tag: key)) {
      Get.delete<Rx<BlackoutStatus>>(tag: key);
    }
  }
}

class BlackoutMenuLabel extends StatelessWidget {
  final BlackoutStatus status;
  final String label;
  final String onText;
  final String offText;

  const BlackoutMenuLabel(
      {Key? key,
      required this.status,
      required this.label,
      required this.onText,
      required this.offText})
      : super(key: key);

  @override
  Widget build(BuildContext context) {
    final Widget state;
    if (status.isPending) {
      state = const SizedBox(
          width: 12,
          height: 12,
          child: CircularProgressIndicator(strokeWidth: 2));
    } else {
      final on = status == BlackoutStatus.on;
      state = Text(on ? onText : offText,
          style: TextStyle(
              fontWeight: FontWeight.bold,
              color: on ? Colors.green : Colors.grey));
    }
    return Row(mainAxisSize: MainAxisSize.min, children: [
      const Icon(Icons.desktop_access_disabled, size: 18),
      const SizedBox(width: 8),
      Text(label),
      const SizedBox(width: 10),
      state,
    ]);
  }
}
