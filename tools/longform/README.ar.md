<div dir="rtl">

# Concat Studio: فيديوهات طويلة من فكرة واحدة

[English](README.md)

أداة تأخذ موضوعاً وتُخرج فيديو جاهزاً، ومعه مشروع Concat الذي تم المونتاج فيه لتعدّله يدوياً إن أردت.

</div>

```
الموضوع ──script──▶ storyboard.json ──prompts──▶ prompts.md
                         │
                         ├──voice────▶ voice/<scene>.wav|mp3
                         ├──visuals──▶ visuals/<scene>_<n>.mp4|png
                         ▼
                       edit ──▶ concat/<title>/  (افتحه في Concat)
                            └─▶ final.mp4
```

<div dir="rtl">

## المراحل

- **script (السكريبت)**: يكتب Claude الفيديو فصلاً فصلاً: مخطط عام أولاً، ثم كل فصل على حدة مع المخطط ونهاية الفصل السابق، حتى يبقى فيديو مدته ساعة متماسكاً. لكل مشهد: نص التعليق الصوتي، وعنوان اختياري على الشاشة، و prompt إنجليزي لكل لقطة مدتها 4 إلى 8 ثوانٍ. يُضاف سطر "الأسلوب البصري" إلى كل prompt لتبدو اللقطات كفيلم واحد.
- **prompts**: كل الـ prompts في ملف واحد مع اسم الملف الذي يجب حفظ كل لقطة به، إن أردت توليد اللقطات يدوياً في أي أداة (Kling، Veo، Runway…).
- **voice (الصوت)**: التعليق الصوتي، ملف لكل مشهد: ElevenLabs، أو أي محرك محلي عبر قالب أمر (Piper، Kokoro…)، أو بدون صوت.
- **visuals (اللقطات)**: أي نموذج text-to-video على Replicate، أو أي مولّد محلي عبر قالب أمر (ComfyUI، Wan، LTX-Video…)، أو لقطات مؤقتة (animatic) لمعاينة الإيقاع، أو يدوياً.
- **edit (المونتاج)**: يقوم به Concat نفسه عبر الـ API: اللقطات متتالية مع انتقالات cross-fade، كل لقطة تُقص أو تُبطّأ أو تُكرر لتملأ مشهدها؛ التعليق الصوتي تحتها؛ موسيقى خلفية متكررة بصوت منخفض مع fade-out؛ عناوين الفصول؛ ترجمة نصية (captions) متزامنة مع التعليق؛ حركة Ken Burns على الصور الثابتة؛ ظهور من الأسود واختفاء إليه. ثم التصدير.

كل مرحلة تتخطى ما هو موجود مسبقاً، فيمكن إعادة تشغيلها بعد أي خطأ، ويمكن استبدال أي ملف يدوياً: احذف لقطة سيئة لإعادة توليدها، أو ضع لقطاتك الخاصة بنفس الاسم. ملف الـ storyboard هو JSON عادي (انظر `example/storyboard.json`) يمكن كتابته أو تعديله يدوياً.

## المتطلبات

- Python 3.10+ (المكتبة القياسية فقط).
- `concat-cli` مبني من `src/`: الأمر `cargo build --profile quick -p concat-cli`.
- `ANTHROPIC_API_KEY` لمرحلة السكريبت.
- مفاتيح المزوّدين الذين تختارهم (`ELEVENLABS_API_KEY`، `REPLICATE_API_TOKEN`) أو أداة محلية.
- `ffmpeg` فقط للّقطات المؤقتة `--shots placeholder`.

## الاستخدام

دفعة واحدة:

</div>

```sh
python3 tools/longform/longform.py run jobs/desert \
  --topic "المدن المفقودة في صحراء الجزيرة العربية" --minutes 12 --language ar \
  --voice elevenlabs --voice-id <id> \
  --shots replicate --replicate-model <owner>/<text-to-video-model> \
  --replicate-input '{"prompt": "{prompt}", "aspect_ratio": "{aspect}"}' \
  --music bed.mp3
```

<div dir="rtl">

أو خطوة بخطوة، مع مراجعة كل مرحلة قبل دفع تكلفة التالية:

</div>

```sh
L=tools/longform/longform.py
python3 $L script  jobs/desert --topic "..." --minutes 12 --language ar
# راجع وعدّل jobs/desert/storyboard.json
python3 $L prompts jobs/desert
python3 $L voice   jobs/desert --voice command \
  --voice-cmd 'piper -m ar_JO-kareem-medium.onnx -f {out} < {text_file}'
python3 $L visuals jobs/desert --shots placeholder   # معاينة الإيقاع أولاً
python3 $L edit    jobs/desert                       # jobs/desert/final.mp4
rm jobs/desert/visuals/*                             # ثم اللقطات الحقيقية
python3 $L visuals jobs/desert --shots command \
  --shot-cmd 'my-generator --prompt {prompt} --seconds {seconds} --size {width}x{height} --out {out}'
python3 $L edit    jobs/desert --music bed.mp3 --force
```

<div dir="rtl">

مرحلة `edit` ترفض الكتابة فوق مشروع موجود لأنه قد يحتوي تعديلاتك اليدوية؛ استخدم `--force` لإعادة بنائه.

باقي الخيارات وتفاصيل الـ timeline في [README.md](README.md).

</div>
