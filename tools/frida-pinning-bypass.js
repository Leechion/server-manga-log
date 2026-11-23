// LeeConnect 抓包期通用 SSL pinning bypass（frida -U -f [PKG] -l tools/frida-pinning-bypass.js）
// 覆盖 native + Java 常见校验点，沙箱 fixture 通用模板。
'use strict';

try {
  // native: OpenSSL 自定义校验返回 OK
  var custom = Module.findExportByName(null, 'SSL_CTX_set_custom_verify');
  if (custom) Interceptor.replace(custom, new NativeCallback(function () { return 0; }, 'int', ['pointer', 'int', 'pointer']));
  var verify = Module.findExportByName(null, 'SSL_get_verify_result');
  if (verify) Interceptor.replace(verify, new NativeCallback(function () { return 0; }, 'int', ['pointer']));
} catch (e) { console.log('native hooks skip:', e.message); }

if (Java && Java.available) {
  Java.perform(function () {
    try {
      var TrustManager = Java.use('javax.net.ssl.X509TrustManager');
      var TM = Java.registerClass({
        name: 'leeconnect.PermissiveTM',
        implements: [TrustManager],
        methods: {
          checkClientTrusted: function () {}, checkServerTrusted: function () {},
          getAcceptedIssuers: function () { return []; }
        }
      });
      var SSL = Java.use('javax.net.ssl.SSLContext');
      SSL.init.overload('[Ljavax.net.ssl.KeyManager;', '[Ljavax.net.ssl.TrustManager;', 'java.security.SecureRandom')
        .implementation = function (km, tm, sr) { return SSL.init(this, km, TM.$new(), sr); };
    } catch (e) { console.log('javax skip:', e.message); }
    try { // OkHttp3 pinning
      var CertPinner = Java.use('okhttp3.CertificatePinner');
      CertPinner.check.overload('java.lang.String', 'java.util.List').implementation = function () {};
    } catch (e) { console.log('okhttp skip:', e.message); }
    try { // 证书链校验
      var CV = Java.use('java.security.cert.CertPathValidator');
      CV.validate.overload('java.security.cert.CertPath', 'java.security.cert.PKIXParameters')
        .implementation = function (p, prm) { return Java.cast(null, Java.use('java.security.cert.CertPathValidatorResult')); };
    } catch (e) { console.log('certpath skip:', e.message); }
  });
}
console.log('[leeconnect] pinning bypass loaded');
