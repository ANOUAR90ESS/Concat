"""Concat Studio: a long video from a topic, cut by Concat.

The pipeline is five steps, each one resumable because each one leaves
files behind and the next step only reads files:

    script    a topic becomes a storyboard (chapters, scenes, narration
              and one visual prompt per shot), written by Claude
    prompts   the storyboard's prompts as a sheet, for generating the
              shots by hand in any tool
    voice     the narration spoken, one file a scene
    visuals   the shots generated, one or more files a scene
    edit      the files cut into a Concat project and exported

Nothing here knows how to decode a frame. The montage is Concat's: the
edit step speaks the Concat API (`concat-cli api`) and every cut, title,
transition and key it makes is one the window could have made, so the
project it leaves behind opens in the editor for a hand finish.
"""
