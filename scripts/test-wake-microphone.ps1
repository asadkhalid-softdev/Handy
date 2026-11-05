# Device matching tests never open a microphone. Optional live test requires
# an explicitly named microphone and tests two simultaneous wake inputs.
param([string]$LiveMicrophone)
$ErrorActionPreference='Stop'
$detector=Join-Path $PSScriptRoot '..\src-tauri\src\voice_control\detector.ps1'
$env:HANDY_WAKE_TEST_PROBE='1'
try { $null = & $detector } finally { Remove-Item Env:HANDY_WAKE_TEST_PROBE -ErrorAction SilentlyContinue }
$full='Microphone Array (Intel Smart Sound Technology)'
$short=$full.Substring(0,31)
if([HandyMicrophoneStream]::SelectDevice($full,@('Other microphone',$short)) -ne 1){throw 'Truncated device matching failed'}
if([HandyMicrophoneStream]::SelectDevice($full,@($short,$full)) -ne 1){throw 'Exact match should take priority'}
$intelFull='Microphone Array (Intel' + [char]0x00ae + ' Smart Sound Technology for Digital Microphones)'
if([HandyMicrophoneStream]::SelectDevice($intelFull,@('Headset (Bluetooth)','Microphone Array (Intelr Smart ')) -ne 1){throw 'Intel legacy device-name normalization failed'}
foreach($case in @(
 @{Name='missing';Names=@('Other microphone')},
 @{Name='ambiguous';Names=@($short,$short)},
 @{Name='ambiguous exact';Names=@($full,$full)}
)) {
 $rejected=$false
 try { $null=[HandyMicrophoneStream]::SelectDevice($full,[string[]]$case.Names) } catch { $rejected=$true }
 if(!$rejected){throw "Should reject $($case.Name) input instead of opening a different mic"}
}
Write-Output 'PASS: exact and truncated device selection; unavailable and ambiguous inputs fail without fallback'
if($LiveMicrophone) {
 $inputs=@()
 $engines=@()
 try {
  $inputs+=New-Object HandyMicrophoneStream($LiveMicrophone)
  $inputs+=New-Object HandyMicrophoneStream($LiveMicrophone)
  foreach($inputStream in $inputs) {
   $buffer=New-Object byte[] 4096
   if($inputStream.Read($buffer,0,$buffer.Length) -ne $buffer.Length){throw 'Live PCM returned a short read, which SAPI treats as EOF'}
  }
  Write-Output 'PASS: selected microphone supports two simultaneous PCM streams'
  foreach($inputStream in $inputs) {
   $engine=New-Object System.Speech.Recognition.SpeechRecognitionEngine([Globalization.CultureInfo]::GetCultureInfo('en-US'))
   $engines+=$engine
   $grammar=New-Object System.Speech.Recognition.GrammarBuilder('handy microphone probe')
   $grammar.Culture=[Globalization.CultureInfo]::GetCultureInfo('en-US')
   $engine.LoadGrammar((New-Object System.Speech.Recognition.Grammar($grammar)))
   $format=New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000,[System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,[System.Speech.AudioFormat.AudioChannel]::Mono)
   $engine.SetInputToAudioStream($inputStream,$format)
   $engine.RecognizeAsync([System.Speech.Recognition.RecognizeMode]::Multiple)
  }
  Start-Sleep -Seconds 2
  foreach($engine in $engines) {
   if($engine.AudioState -eq [System.Speech.Recognition.AudioState]::Stopped){throw 'Speech recognizer stopped reading the selected microphone'}
  }
  Write-Output 'PASS: both speech recognizers remain active on the selected microphone'
 } finally {
  foreach($inputStream in $inputs){$inputStream.Dispose()}
  foreach($engine in $engines){$engine.Dispose()}
 }
}
