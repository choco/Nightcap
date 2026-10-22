import { Desk } from './Desk';
import { Section, SectionText } from './Section';

export function How({ shiftOn }: { shiftOn: boolean }) {
  return (
    <Section id="how" className="flex flex-col items-center gap-8">
      <div className="flex max-w-[720px] flex-col items-center gap-3 text-center" data-lens-area>
        <h2>It follows what's in front</h2>
        <SectionText>
          Pick the apps, and the websites in Safari, Chrome, Firefox and more, that need true colour. While one is in front, Night Shift stays off. Switch away and it comes back, and your schedule carries on as before. Rules work the other way too: keep Night Shift on for the app you read in at night, even with it off everywhere else.
        </SectionText>
      </div>
      <Desk shiftOn={shiftOn} />
    </Section>
  );
}
